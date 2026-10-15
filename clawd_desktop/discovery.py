"""Find the Clawd clock on the LAN, so nobody has to type an IP address more than once.

Order of preference, re-evaluated every couple of seconds:

1. A manual address: `http_targets` in the config file, or one typed in the tray
   menu ("Nhập địa chỉ đồng hồ…") or the settings window. Used as-is.
2. The remembered clock (clock.json), looked up by its mDNS name, so a new IP
   from the router is followed automatically. Its last known address is tried
   right away at startup, before mDNS has answered (the ESP8266 takes ~6 s).
3. The first clock that answers mDNS (`_clawd._tcp`, advertised by the
   firmware with TXT id=<hostname> ver=<version>). It becomes the remembered one.

There is no port scan. Where mDNS does not reach the clock (an office whose wired
PCs and WiFi devices sit on different subnets, a guest network), the remembered
address is the only host ever tried, and when it stops answering the user types
the IP the clock shows on its screen.

Push results feed back through `report()`: a clock that fails FAIL_LIMIT pushes
in a row is dropped for BAD_HOLD seconds, then its address is tried again.
"""
from __future__ import annotations

import asyncio
import contextlib
import ipaddress
import json
import re
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

SERVICE = "_clawd._tcp.local."
FAIL_LIMIT = 3            # consecutive failed pushes before a clock is dropped
BAD_HOLD = 60.0           # seconds a dropped clock is ignored before its address is tried again
TICK = 2.0
RESOLVE_TIMEOUT_MS = 4000
_LABEL = re.compile(r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?")


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
    if not valid_host(host):
        return None
    try:
        p = int(port) if port else 80
    except ValueError:
        return None
    if not 0 < p < 65536:
        return None
    return Clock(host=host, port=p, source="manual")


def valid_host(host: str) -> bool:
    """A full IPv4 address or a DNS name (`clawd.local`). Rejects a typo such as
    `10..97.40.207` (the resolver's IDNA codec raises on its empty label) and a short
    dotted number such as `10.97.40` (the resolver would read it as 10.97.0.40)."""
    if not host or len(host) > 253:
        return False
    labels = host.split(".")
    if all(label.isdigit() for label in labels):
        try:
            ipaddress.IPv4Address(host)
        except ValueError:
            return False
        return True
    return all(_LABEL.fullmatch(label) for label in labels)


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

    def unpin(self) -> Clock | None:
        """Turn a typed address into a remembered one (mDNS may now replace it) and return
        the clock to try; None when nothing was ever saved."""
        data = self.load()
        typed = manual_clock(str(data.get("manual_host") or ""))
        if typed is not None:
            data = {"name": "", "host": typed.host, "port": typed.port, "version": ""}
            self.save(data)
        return stored_clock(data)


def stored_clock(data: dict) -> Clock | None:
    """The clock clock.json points at: the typed address (pinned), else the remembered one."""
    typed = manual_clock(str(data.get("manual_host") or ""))
    if typed is not None:
        return typed
    if not data.get("host"):
        return None
    try:
        port = int(data.get("port") or 80)
    except (TypeError, ValueError):
        port = 80
    return Clock(host=str(data["host"]), port=port, name=str(data.get("name") or ""),
                 version=str(data.get("version") or ""), source="saved")


# =============================================================================
# mDNS (zeroconf, in its own threads)
# =============================================================================
class MdnsBrowser:
    """Keeps a live set of `_clawd._tcp` instance names; resolves on demand."""

    def __init__(self) -> None:
        self._names: set[str] = set()
        self._lock = threading.Lock()
        self._zc = None
        self._browser = None
        self.changed = 0          # bumps on every add/update/remove

    def start(self) -> bool:
        try:
            from zeroconf import IPVersion, ServiceBrowser, ServiceListener, Zeroconf
        except ImportError:
            log("zeroconf not installed: mDNS discovery off")
            return False
        outer = self

        class Listener(ServiceListener):
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
            with contextlib.suppress(Exception):   # zeroconf may already be half closed
                self._zc.close()
            self._zc = None


# =============================================================================
# The finder
# =============================================================================
class ClockFinder:
    def __init__(self, store: ClockStore,
                 manual: Callable[[], list[str]] = list,
                 browser: MdnsBrowser | None = None,
                 clock: Callable[[], float] = time.monotonic) -> None:
        self.store = store
        self.manual = manual              # `http_targets` from the config file
        self.browser = browser if browser is not None else MdnsBrowser()
        self.clock = clock
        self.failures = 0
        self.last_ok: float | None = None     # wall time of the last accepted push
        self.searching = True
        self._bad: dict[str, float] = {}       # host -> retry after (monotonic)
        self._seen_change = -1
        self._mdns: dict[str, Clock] = {}       # instance name -> resolved clock
        self._lock = threading.Lock()
        # Tried at once (mDNS confirms or corrects it shortly): typed address, else the remembered one.
        self.current: Clock | None = stored_clock(store.load())

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
                if cur.source == "saved":
                    self._remember(cur)
                return
            self.failures += 1
            if self.failures >= FAIL_LIMIT and cur.source != "manual":
                log(f"Clock {cur.label} not answering; trying its address again in {BAD_HOLD:.0f} s")
                self._bad[cur.host] = self.clock() + BAD_HOLD
                self.current = None
                self.failures = 0

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

    def use(self, c: Clock) -> None:
        """The clock the user picked from the ones mDNS sees (setup wizard): remembered by
        name, not pinned, so a new DHCP address follows it."""
        with self._lock:
            self.store.save({"name": c.name, "host": c.host, "port": c.port, "version": c.version})
            self.current, self.failures = c, 0
            self._bad.pop(c.host, None)
        log(f"Clock picked: {c.label}")

    def retry(self) -> None:
        """'Tìm lại': keep the address, try it again now and let mDNS replace it. A typed
        address loses its pin (it would win over mDNS forever), not its value."""
        with self._lock:
            c = self.store.unpin()
            self.current, self.failures = c, 0
            self._bad.clear()
            self._mdns.clear()
            self._seen_change = -1
        log(f"Trying the clock again: {c.label}" if c else "Looking for a clock (mDNS only, no address yet)")

    # ---- the loop -------------------------------------------------------------
    def _remember(self, c: Clock) -> None:
        data = self.store.load()
        if data.get("manual_host"):
            return
        # A clock without a name (typed once) keeps the old name only for the same address.
        name = c.name or (data.get("name", "") if data.get("host") == c.host else "")
        new = {"name": name, "host": c.host, "port": c.port, "version": c.version}
        if {k: data.get(k) for k in new} != new or set(data) - set(new):   # also drops old keys
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
            elif cur is not None and cur.source == "saved":
                # An address without a name: take the mDNS name once it shows up there.
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

    async def step(self) -> None:
        if self.manual():
            self.searching = False
            return
        await self._refresh_mdns()
        self.searching = not self._choose(self.clock())

    async def run(self, stop: asyncio.Event) -> None:
        self.browser.start()
        try:
            while not stop.is_set():
                try:
                    await self.step()
                except Exception as e:  # keep looking; the next tick retries
                    log(f"Clock discovery step failed: {e!r}")
                with contextlib.suppress(asyncio.TimeoutError):
                    await asyncio.wait_for(stop.wait(), timeout=TICK)
        finally:
            await asyncio.to_thread(self.browser.stop)
