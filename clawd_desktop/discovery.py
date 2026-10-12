"""Find the Clawd clock on the LAN, so nobody has to type an IP address.

Order of preference, re-evaluated every couple of seconds:

1. A manual address: `http_targets` in the config file, or one typed in the tray
   menu ("Nhập địa chỉ đồng hồ…"). Used as-is, discovery stays out of the way.
2. The remembered clock (clock.json), looked up by its mDNS name, so a new IP
   from the router is followed automatically. Its last known address is tried
   right away at startup, before mDNS has answered (the ESP8266 takes ~6 s).
3. The first clock that answers mDNS (`_clawdmeter._tcp`, advertised by the
   firmware with TXT id=<hostname> ver=<version>). It becomes the remembered one.
4. When mDNS stays silent (mesh routers and guest networks often drop
   multicast), a scan of the local /24 for a host whose `GET /api/heap` looks
   like the clawd firmware.

Push results feed back through `report()`: a clock that fails FAIL_LIMIT pushes
in a row is dropped for BAD_HOLD seconds, so the app stops polling the usage
API for a screen that is not there and looks for it again.
"""
from __future__ import annotations

import asyncio
import errno
import ipaddress
import json
import socket
import threading
import time
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Callable, Iterable

import httpx

SERVICE = "_clawdmeter._tcp.local."
FAIL_LIMIT = 3            # consecutive failed pushes before a clock is dropped
BAD_HOLD = 60.0           # seconds a dropped clock is ignored before it is retried
SCAN_AFTER = 15.0         # seconds without any clock before the LAN scan starts
RESCAN_EVERY = 300.0      # minimum gap between two LAN scans
RESCAN_BLOCKED = 30.0     # ... while the OS blocks the LAN (macOS Local Network permission)
TICK = 2.0
RESOLVE_TIMEOUT_MS = 4000
SCAN_CONNECT_TIMEOUT = 0.6
SCAN_HTTP_TIMEOUT = 12.0  # the clock answers HTTP slowly for a few minutes after boot
SCAN_CONCURRENCY = 96


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


@dataclass(frozen=True)
class Clock:
    host: str
    port: int = 80
    name: str = ""          # mDNS id (the clock's hostname); "" when found by scan or typed
    version: str = ""
    source: str = "mdns"    # mdns | scan | saved | manual

    @property
    def base(self) -> str:
        return f"http://{self.host}" + ("" if self.port == 80 else f":{self.port}")

    @property
    def usage_url(self) -> str:
        return self.base + "/api/usage"

    @property
    def label(self) -> str:
        return f"{self.name} ({self.host})" if self.name else self.host


def manual_clock(raw: str) -> Clock | None:
    """`192.168.1.48`, `clawd.local:8080` or `http://10.0.0.5` -> a manual Clock."""
    raw = raw.strip()
    if not raw:
        return None
    if "://" in raw:
        raw = raw.split("://", 1)[1]
    raw = raw.split("/", 1)[0]
    host, _, port = raw.partition(":")
    if not host:
        return None
    try:
        p = int(port) if port else 80
    except ValueError:
        return None
    if not 0 < p < 65536:
        return None
    return Clock(host=host, port=p, source="manual")


# =============================================================================
# Persistence: ~/.config/claude-usage-monitor/clock.json
# =============================================================================
class ClockStore:
    def __init__(self, path: Path) -> None:
        self.path = path

    def load(self) -> dict:
        try:
            data = json.loads(self.path.read_text())
            return data if isinstance(data, dict) else {}
        except (OSError, ValueError):
            return {}

    def save(self, data: dict) -> None:
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            tmp = self.path.with_suffix(".tmp")
            tmp.write_text(json.dumps(data, indent=2))
            tmp.replace(self.path)
        except OSError as e:
            log(f"Could not save {self.path}: {e}")


# =============================================================================
# mDNS (zeroconf, in its own threads)
# =============================================================================
class MdnsBrowser:
    """Keeps a live set of `_clawdmeter._tcp` instance names; resolves on demand."""

    def __init__(self) -> None:
        self._names: set[str] = set()
        self._lock = threading.Lock()
        self._zc = None
        self._browser = None
        self.changed = 0          # bumps on every add/update/remove

    def start(self) -> bool:
        try:
            from zeroconf import IPVersion, ServiceBrowser, Zeroconf
        except ImportError:
            log("zeroconf not installed: mDNS discovery off")
            return False
        outer = self

        class Listener:
            def add_service(self, zc, type_, name):
                with outer._lock:
                    outer._names.add(name)
                    outer.changed += 1

            def update_service(self, zc, type_, name):
                with outer._lock:
                    outer._names.add(name)
                    outer.changed += 1

            def remove_service(self, zc, type_, name):
                with outer._lock:
                    outer._names.discard(name)
                    outer.changed += 1

        try:
            self._zc = Zeroconf(ip_version=IPVersion.V4Only)
            self._browser = ServiceBrowser(self._zc, SERVICE, Listener())
        except OSError as e:     # no multicast-capable interface, port in use...
            log(f"mDNS discovery unavailable: {e}")
            self._zc = None
            return False
        return True

    def names(self) -> list[str]:
        with self._lock:
            return sorted(self._names)

    def resolve(self, name: str) -> Clock | None:
        """Blocking (run it in a thread): the address and TXT of one instance."""
        if self._zc is None:
            return None
        try:
            info = self._zc.get_service_info(SERVICE, name, timeout=RESOLVE_TIMEOUT_MS)
        except Exception as e:  # zeroconf raises a few odd types on a closing socket
            log(f"mDNS resolve of {name} failed: {e!r}")
            return None
        if info is None:
            return None
        addrs = info.parsed_addresses()
        if not addrs:
            return None
        props = {k.decode(errors="replace"): (v or b"").decode(errors="replace")
                 for k, v in (info.properties or {}).items()}
        ident = props.get("id") or name.split(".", 1)[0]
        return Clock(host=addrs[0], port=info.port or 80, name=ident,
                     version=props.get("ver", ""), source="mdns")

    def stop(self) -> None:
        if self._zc is not None:
            try:
                self._zc.close()
            except Exception:
                pass
            self._zc = None


# =============================================================================
# LAN scan fallback
# =============================================================================
def local_ipv4() -> str | None:
    """The address this machine uses to reach the LAN (no packet is sent)."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("10.255.255.255", 1))
        ip = s.getsockname()[0]
    except OSError:
        return None
    finally:
        s.close()
    return None if ip.startswith("127.") else ip


def scan_candidates(ip: str | None) -> list[str]:
    """Every other host in the /24 around `ip` (most home networks)."""
    if not ip:
        return []
    try:
        net = ipaddress.ip_network(f"{ip}/24", strict=False)
    except ValueError:
        return []
    return [str(h) for h in net.hosts() if str(h) != ip]


def looks_like_clawd(body: object) -> bool:
    """`GET /api/heap` of the clawd firmware: unauthenticated, has the pack counters."""
    return isinstance(body, dict) and "desk" in body and "pak" in body and "version" in body


class ScanResult(list):
    """The clocks found, plus whether the OS refused to let us reach the LAN at all."""
    blocked = False


def lan_blocked(errors: list[int | None]) -> bool:
    """True when (almost) every probe failed at once with "no route to host".

    That is how macOS answers while the app lacks the Local Network permission
    (prompt not answered yet, or denied). On a normal LAN, absent hosts time out
    and the router answers or refuses."""
    if not errors:
        return False
    if any(e == 0 or e == errno.ECONNREFUSED for e in errors):
        return False
    unreach = sum(1 for e in errors if e in (errno.EHOSTUNREACH, errno.ENETUNREACH, errno.EPERM))
    return unreach >= 0.9 * len(errors)


async def _port_open(host: str, port: int, sem: asyncio.Semaphore,
                     errors: list[int | None] | None = None) -> bool:
    async with sem:
        try:
            _r, w = await asyncio.wait_for(asyncio.open_connection(host, port),
                                           timeout=SCAN_CONNECT_TIMEOUT)
        except asyncio.TimeoutError:
            if errors is not None:
                errors.append(None)
            return False
        except OSError as e:
            if errors is not None:
                errors.append(e.errno)
            return False
        if errors is not None:
            errors.append(0)
        w.close()
        try:
            await w.wait_closed()
        except OSError:
            pass
        return True


async def scan_lan(hosts: Iterable[str] | None = None,
                   transport: httpx.AsyncBaseTransport | None = None) -> ScanResult:
    """Clawd clocks in the local /24, found by a port-80 sweep then /api/heap."""
    hosts = list(hosts) if hosts is not None else scan_candidates(local_ipv4())
    found = ScanResult()
    if not hosts:
        return found
    sem = asyncio.Semaphore(SCAN_CONCURRENCY)
    if transport is None:
        errors: list[int | None] = []
        open_ = await asyncio.gather(*(_port_open(h, 80, sem, errors) for h in hosts))
        found.blocked = lan_blocked(errors)
        hosts = [h for h, ok in zip(hosts, open_) if ok]
    async with httpx.AsyncClient(timeout=SCAN_HTTP_TIMEOUT, transport=transport) as http:
        async def probe(h: str) -> None:
            try:
                r = await http.get(f"http://{h}/api/heap")
                body = r.json() if r.status_code == 200 else None
            except (httpx.HTTPError, ValueError):
                return
            if looks_like_clawd(body):
                found.append(Clock(host=h, version=str(body.get("version", "")), source="scan"))
        await asyncio.gather(*(probe(h) for h in hosts))
    found.sort(key=lambda c: tuple(int(p) if p.isdigit() else 0 for p in c.host.split(".")))
    return found


# =============================================================================
# The finder
# =============================================================================
class ClockFinder:
    def __init__(self, store: ClockStore,
                 manual: Callable[[], list[str]] = lambda: [],
                 browser: MdnsBrowser | None = None,
                 scanner: Callable[[], "asyncio.Future[list[Clock]]"] | None = None,
                 clock: Callable[[], float] = time.monotonic) -> None:
        self.store = store
        self.manual = manual              # `http_targets` from the config file
        self.browser = browser if browser is not None else MdnsBrowser()
        self.scanner = scanner or scan_lan
        self.clock = clock
        self.current: Clock | None = None
        self.failures = 0
        self.last_ok: float | None = None     # wall time of the last accepted push
        self.searching = True
        self.scanning = False
        self.lan_blocked = False
        self._bad: dict[str, float] = {}       # host -> retry after (monotonic)
        self._started = clock()
        self._last_scan = -1e9
        self._seen_change = -1
        self._mdns: dict[str, Clock] = {}       # instance name -> resolved clock
        self._lock = threading.Lock()
        saved = store.load()
        typed = manual_clock(saved.get("manual_host", "")) if saved.get("manual_host") else None
        if typed:
            self.current = typed
        elif saved.get("host"):
            # Try the last address at once; mDNS confirms or corrects it shortly.
            self.current = Clock(host=saved["host"], port=int(saved.get("port") or 80),
                                 name=saved.get("name", ""), version=saved.get("version", ""),
                                 source="saved")

    # ---- what the publishers push to --------------------------------------
    def targets(self) -> list[str]:
        cfg = self.manual()
        if cfg:
            return cfg
        with self._lock:
            return [self.current.usage_url] if self.current else []

    def describe(self) -> dict:
        """Snapshot for the tray menu."""
        with self._lock:
            cfg = self.manual()
            return {
                "manual_config": bool(cfg),
                "clock": None if cfg else self.current,
                "config_targets": cfg,
                "last_ok": self.last_ok,
                "failing": self.failures >= FAIL_LIMIT or (self.current is None and not cfg),
                "scanning": self.scanning,
                "lan_blocked": self.lan_blocked and self.current is None,
            }

    # ---- feedback from the usage pushes -----------------------------------
    def report(self, url: str, ok: bool) -> None:
        with self._lock:
            cur = self.current
            if cur is None or url != cur.usage_url:
                if ok:
                    self.last_ok = time.time()
                return
            if ok:
                self.failures = 0
                self.last_ok = time.time()
                if cur.source in ("saved", "scan"):
                    self._remember(cur)
                return
            self.failures += 1
            if self.failures >= FAIL_LIMIT and cur.source != "manual":
                log(f"Clock {cur.label} not answering; looking for it again")
                self._bad[cur.host] = self.clock() + BAD_HOLD
                self.current = None
                self.failures = 0
                self._last_scan = -1e9     # allow a scan right away if mDNS is silent

    # ---- tray actions --------------------------------------------------------
    def set_manual(self, raw: str) -> Clock | None:
        c = manual_clock(raw)
        if c is None:
            return None
        with self._lock:
            data = self.store.load()
            data["manual_host"] = raw.strip()
            self.store.save(data)
            self.current, self.failures = c, 0
        log(f"Clock set by hand: {c.label}")
        return c

    def forget(self) -> None:
        """'Tìm lại đồng hồ': drop the remembered and typed clock and search afresh."""
        with self._lock:
            self.store.save({})
            self.current, self.failures = None, 0
            self._bad.clear()
            self._mdns.clear()
            self._seen_change = -1
            self._last_scan = -1e9
            self._started = self.clock()
        log("Forgot the clock; searching again")

    # ---- the loop -------------------------------------------------------------
    def _remember(self, c: Clock) -> None:
        data = self.store.load()
        if data.get("manual_host"):
            return
        # A clock found by scan has no name: keep the old one only for the same address.
        name = c.name or (data.get("name", "") if data.get("host") == c.host else "")
        new = {"name": name, "host": c.host, "port": c.port, "version": c.version}
        if {k: data.get(k) for k in new} != new:
            self.store.save(new)

    def _adopt(self, c: Clock, why: str) -> None:
        if self.current != c:
            log(f"Clock {why}: {c.label}" + (f" v{c.version}" if c.version else ""))
        self.current, self.failures = c, 0
        self._remember(c)

    async def _refresh_mdns(self) -> None:
        names = self.browser.names()
        change = self.browser.changed
        if change == self._seen_change and all(n in self._mdns for n in names):
            return
        self._seen_change = change
        fresh: dict[str, Clock] = {}
        for n in names:
            c = await asyncio.to_thread(self.browser.resolve, n)
            if c is not None:
                fresh[n] = c
        with self._lock:
            self._mdns = fresh
            # A fresh mDNS answer clears a hold on that address.
            for c in fresh.values():
                self._bad.pop(c.host, None)

    def _choose(self, now: float) -> bool:
        """Pick a clock from what mDNS knows. True when one is in use."""
        with self._lock:
            cur = self.current
            if cur is not None and cur.source == "manual":
                return True
            saved = self.store.load()
            want = saved.get("name") or (cur.name if cur else "")
            mdns = [c for c in self._mdns.values() if self._bad.get(c.host, 0) <= now]
            if want:
                match = next((c for c in mdns if c.name == want), None)
                if match is not None:
                    if cur is None or (cur.host, cur.port, cur.source) != (match.host, match.port, "mdns"):
                        self._adopt(match, "found" if cur is None else "address updated")
                    return True
            elif cur is not None and cur.source in ("scan", "saved"):
                # Found by scan: take the mDNS name once its address shows up there.
                match = next((c for c in mdns if c.host == cur.host), None)
                if match is not None:
                    self._adopt(match, "named")
                return True
            if cur is not None:
                return True
            if self._bad.get(saved.get("host", ""), 0) <= now and saved.get("host"):
                self._adopt(Clock(host=saved["host"], port=int(saved.get("port") or 80),
                                  name=saved.get("name", ""), source="saved"), "retrying")
                return True
            if mdns and not want:
                self._adopt(sorted(mdns, key=lambda c: c.name)[0], "found")
                return True
            return False

    async def _scan(self, now: float) -> None:
        self._last_scan = now
        self.scanning = True
        log("No clock answered mDNS; scanning the local network")
        try:
            found = await self.scanner()
        finally:
            self.scanning = False
        blocked = bool(getattr(found, "blocked", False))
        if blocked and not self.lan_blocked:
            log("The OS blocks access to the local network (macOS: Local Network permission)")
        self.lan_blocked = blocked
        with self._lock:
            if self.current is not None:
                return
            saved = self.store.load()
            ok = [c for c in found if self._bad.get(c.host, 0) <= self.clock()]
            pick = next((c for c in ok if c.host == saved.get("host")), ok[0] if ok else None)
            if pick is not None:
                self._adopt(replace(pick, name=saved.get("name", "") if pick.host == saved.get("host") else ""),
                            "found by scan")
            else:
                log("LAN scan found no clock")

    async def step(self) -> None:
        if self.manual():
            self.searching = False
            return
        now = self.clock()
        await self._refresh_mdns()
        have = self._choose(now)
        self.searching = not have
        if (not have and now - self._started >= SCAN_AFTER
                and now - self._last_scan >= (RESCAN_BLOCKED if self.lan_blocked else RESCAN_EVERY)):
            await self._scan(now)
            self.searching = self.current is None

    async def run(self, stop: asyncio.Event) -> None:
        self.browser.start()
        try:
            while not stop.is_set():
                try:
                    await self.step()
                except Exception as e:  # keep looking; the next tick retries
                    log(f"Clock discovery step failed: {e!r}")
                try:
                    await asyncio.wait_for(stop.wait(), timeout=TICK)
                except asyncio.TimeoutError:
                    pass
        finally:
            await asyncio.to_thread(self.browser.stop)
