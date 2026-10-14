"""What the settings window's page can ask Python to do (pywebview `js_api`).

Plain Python with no pywebview import, so it is unit-tested directly. pywebview
exposes every public method of this object to the page and skips names that
start with "_", so all state and helpers here are private. The page is treated
as untrusted input: a save is filtered through SCHEMA, and the web password
never goes back to it.

Every method returns a JSON-able dict: {"ok": true, ...} or
{"ok": false, "code": "auth" | "offline" | "bad" | "busy" | "error", "error": "<Vietnamese text>"}.
"""
from __future__ import annotations

import contextlib
import copy
import json
import re
import subprocess
import threading
import time
import webbrowser
from base64 import b64encode
from collections.abc import Callable
from pathlib import Path
from typing import Any

import httpx

import clawd_desktop.preview as pv
from clawd_desktop import __version__, agents, appupdate, autostart, creds, feed as fd, firmware, firstrun, hooks, ota, paths
from clawd_desktop.clockapi import AuthNeeded, ClockApi, ClockError, config_diff
from clawd_desktop.discovery import Clock, ClockStore, MdnsBrowser, manual_clock, stored_clock

FEED_RECHECK = 600      # the window asks the feed again after this long (s)
PAK_RETRY_S = 30        # after a failed /clawd.pak read, wait this long before asking again
SEARCH_WAIT = 15        # "Tìm lại" waits this long (s): the saved address again, or an mDNS answer
PROBE_TIMEOUT = 3.0     # one quick GET /api/heap to that one address per try (never a scan)
TYPE_IP = "Nhập IP hiện trên màn hình đồng hồ (lúc khởi động) vào ô Nhập địa chỉ rồi bấm Dùng."
HOSTNAME = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,30}[a-z0-9])?$")
HHMM = re.compile(r"^(?:[01]\d|2[0-3]):[0-5]\d$")


def _int(lo: int, hi: int):
    def f(v):
        if isinstance(v, bool) or not isinstance(v, int) or not lo <= v <= hi:
            raise ValueError(f"needs a whole number {lo}..{hi}")
        return v
    return f


def _bool(v):
    if not isinstance(v, bool):
        raise ValueError("needs true/false")
    return v


def _str(maxlen: int, rx: re.Pattern | None = None, minlen: int = 0):
    def f(v):
        if not isinstance(v, str) or not minlen <= len(v) <= maxlen or (rx and not rx.match(v)):
            raise ValueError("bad text")
        return v
    return f


def _wifi(v):
    if not isinstance(v, list) or not 1 <= len(v) <= 4:
        raise ValueError("1 to 4 networks")
    out, seen = [], set()
    for row in v:
        if not isinstance(row, dict):
            raise ValueError("bad network row")
        ssid = _str(32, minlen=1)(row.get("ssid"))
        if ssid in seen:
            raise ValueError(f"network {ssid} twice")
        seen.add(ssid)
        r = {"ssid": ssid}
        pw = row.get("pass")
        if pw:                          # blank keeps the stored password (firmware rule)
            r["pass"] = _str(64, minlen=8)(pw)
        out.append(r)
    return out


# The settings the window edits. Anything else the page sends is dropped.
SCHEMA: dict[str, Any] = {
    "hostname": _str(32, HOSTNAME, 1),
    "brightness": _int(0, 100),
    "rotation": _int(0, 3),
    "wifi": _wifi,
    "usage": {"screen": _int(0, 2), "anim": _int(-1, 63), "layout": _int(0, 15)},
    "clock": {"tz": _str(48), "tzPosix": _str(64), "nightEnabled": _bool,
              "nightStart": _str(5, HHMM), "nightEnd": _str(5, HHMM), "nightLevel": _int(0, 100)},
}


def clean(data: Any, schema: dict = SCHEMA, strict: bool = True) -> dict:
    """The part of `data` the schema knows, checked. `strict=False` (for the
    clock's own config) skips values that fail instead of raising."""
    out: dict = {}
    if not isinstance(data, dict):
        return out
    for k, rule in schema.items():
        if k not in data:
            continue
        try:
            out[k] = clean(data[k], rule, strict) if isinstance(rule, dict) else rule(data[k])
        except ValueError as e:
            if strict:
                raise ValueError(f"{k}: {e}") from e
    return out


def _clock_view(cfg: dict) -> dict:
    """The clock's config in the shape the window saves (wifi rows lose passSet)."""
    view = dict(cfg)
    if isinstance(cfg.get("wifi"), list):
        view["wifi"] = [{"ssid": r.get("ssid")} for r in cfg["wifi"] if isinstance(r, dict) and r.get("ssid")]
    return clean(view, strict=False)


def _fail(code: str, msg: str) -> dict:
    return {"ok": False, "code": code, "error": msg}


def _err(e: Exception) -> dict:
    if isinstance(e, AuthNeeded):
        return _fail("auth", "Đồng hồ cần mật khẩu trang cài đặt.")
    if isinstance(e, ClockError) and str(e).startswith("no answer"):
        return _fail("offline", "Đồng hồ không trả lời. Đồng hồ còn bật và cùng mạng Wi‑Fi không?")
    return _fail("error", f"Lỗi: {e}")


class SettingsApi:
    def __init__(self, base: str, key: str, *,
                 notify: Callable[[dict], bool | None] = lambda _m: None,
                 updater: appupdate.Updater | None = None,
                 api_factory: Callable[..., ClockApi] = ClockApi,
                 bundle: firmware.Bundle | None = None,
                 feed: fd.Feed | None = None,
                 store: ClockStore | None = None,
                 keychain=creds,
                 remember_screen: Callable[[int], None] = paths.set_clock_screen,
                 renderer: pv.Renderer | None = None,
                 renderer_dir: Path | None = None,
                 payload_source: Callable[[], tuple[dict, float] | None] = paths.usage_payload,
                 now: Callable[[], float] = time.time, tab: str = "",
                 agents_reg: dict | None = None, mdns_factory: Callable[[], MdnsBrowser] = MdnsBrowser) -> None:
        self._mdns_factory = mdns_factory
        self._tab = tab if tab in ("update", "agents", "setup") else ""   # the page opens on this tab once
        self._renderer = renderer              # draws the clock's screen (None: the page approximates)
        self._renderer_dir = renderer_dir or paths.RENDERER_DIR   # where a feed release's renderer is unpacked
        self._payload_source = payload_source  # the last usage payload the engine sent
        self._now = now
        self._frames = pv.FrameCache()
        self._pak_failed = 0.0                 # when reading /clawd.pak off the clock last failed
        self._remember_screen = remember_screen  # tell the engine the main screen
        self._notify = notify                  # tell the tray (rescan, set host, refresh); True: delivered
        self._updater = updater                # updates of the app itself (appupdate.py), None: not offered
        self._factory = api_factory
        self._bundle = bundle
        self._feed = feed                      # newer firmware + packs than the bundle (feed.py)
        self._release: fd.Release | None = None
        self._feed_at = 0.0                    # last feed check (monotonic)
        self._feed_error = ""
        self._feed_lock = threading.Lock()     # one feed check at a time
        self._feed_pending = False             # a background check is running (the page waits for it)
        self._feed_thread: threading.Thread | None = None
        self._store = store or ClockStore(paths.CLOCK_FILE)
        self._keychain = keychain
        self._window: Any = None               # set by settings_app, for file dialogs
        self._lock = threading.Lock()
        self._job: dict = {"running": False}
        self._heap: dict = {}
        self._cfg: dict = {}
        self._agents = agents_reg                # the coding agents (agents.registry())
        self._mdns: MdnsBrowser | None = None    # started by the wizard's clock step
        self._mdns_seen: dict[str, Clock] = {}   # instance name -> resolved clock
        self._connect(base, key)

    # ---- plumbing --------------------------------------------------------------
    def _connect(self, base: str, key: str) -> None:
        self._base, self._key = base.rstrip("/"), key or base
        self._api = self._factory(self._base, auth=self._keychain.get(self._key) if self._key else None)

    def _follow_store(self) -> bool:
        """The window gets its clock once, when it opens; the tray keeps looking and saves
        where it finds the clock (another network, a new DHCP lease) in clock.json. Use that
        address when it differs from ours. True if it switched."""
        c = stored_clock(self._store.load())
        if c is None or c.base.rstrip("/") == self._base:
            return False
        self._connect(c.base, c.name or self._key)    # the same clock keeps its saved login
        return True

    @property
    def _src(self) -> firmware.Bundle | None:
        """What an update installs and the gallery offers: the feed's release when it is
        newer than the bundled firmware, else the bundle."""
        return self._release or self._bundle

    def _check_feed(self, force: bool = False) -> None:
        """Ask the feed for its newest release (at most every FEED_RECHECK s) and keep it,
        with its style packs, when it is newer than the bundle. Failures only show as text."""
        feed = self._feed
        if feed is None or self._job["running"]:
            return
        if not self._feed_lock.acquire(blocking=force):    # the background check is on it
            return
        try:
            self._check_feed_locked(feed, force)
        finally:
            self._feed_lock.release()

    def _feed_due(self) -> bool:
        return self._feed is not None and not self._job["running"] and not (
            self._feed_at and time.monotonic() - self._feed_at < FEED_RECHECK)

    def _kick_feed(self) -> None:
        """Check the feed in the background: reading the packs takes seconds, and the window
        must show the clock (and the packs the app carries) without waiting for them."""
        if self._feed_pending or not self._feed_due():
            return
        self._feed_pending = True

        def run() -> None:
            try:
                self._check_feed()
            finally:
                self._feed_pending = False

        self._feed_thread = threading.Thread(target=run, name="clawd-feed-check", daemon=True)
        self._feed_thread.start()

    def _check_feed_locked(self, feed: fd.Feed, force: bool) -> None:
        now = time.monotonic()
        if not force and self._feed_at and now - self._feed_at < FEED_RECHECK:
            return
        self._feed_at = now
        try:
            m = feed.latest()
            self._feed_error = ""
        except fd.FeedError as e:
            self._feed_error = str(e)
            print(f"[settings] {e}", flush=True)
            return
        if m is None or (self._bundle is not None and m.seq <= self._bundle.seq):
            self._release = None
            return
        if self._release is not None and self._release.seq == m.seq:
            return
        rel = fd.make_release(m)
        try:                                   # the gallery needs the packs; a few hundred KB
            rel = feed.fetch(rel, fd.STYLE_KINDS)
        except fd.FeedError as e:
            print(f"[settings] style packs from the feed: {e}", flush=True)
        self._release = rel
        self._use_feed_renderer(feed, rel)

    def _use_feed_renderer(self, feed: fd.Feed, rel: fd.Release) -> None:
        """Preview with the renderer of the release the gallery now shows (its packs), when it
        has one for this computer: the one unpacked before if it still checks out, else
        downloaded once. Without it the page keeps what it had (a bundled renderer, or its
        canvas)."""
        m, plat = rel.manifest, fd.renderer_platform()
        f = m.renderer(plat) if m else None
        if m is None or f is None:
            return
        commit = m.commit.removesuffix("-dirty")
        r = pv.load_unpacked(self._renderer_dir, f.sha256, commit)
        if r is None:
            try:
                got = feed.fetch_renderer(rel, plat)
                if got is None:
                    return
                pv.unpack_renderer(got[1], self._renderer_dir, f.sha256)
            except (fd.FeedError, OSError, ValueError) as e:
                print(f"[settings] renderer from the feed: {e}", flush=True)
                return
            r = pv.load_unpacked(self._renderer_dir, f.sha256, commit)
            if r is None:
                print(f"[settings] renderer from the feed (release {m.seq}) does not match it", flush=True)
                return
        # Both at once, in this order of reads in usage_preview: a frame of the renderer before
        # never lands in this one's cache.
        self._frames = pv.FrameCache()
        self._renderer = r
        print(f"[settings] preview: feed renderer {r.commit or '?'} (release {m.seq}, {len(r.styles)} styles)",
              flush=True)

    def _bundle_info(self) -> dict | None:
        b = self._src
        if b is None:
            return None
        out = {"seq": b.seq, "size": b.size, "commit": b.commit, "styleApi": b.style_api,
               "source": "feed" if isinstance(b, fd.Release) else "app"}
        if isinstance(b, fd.Release) and b.manifest is not None:
            out.update(notes=b.notes, date=b.date[:10],
                       packs={f.kind: f.size for f in b.manifest.files if f.kind in fd.PACK_KINDS})
        return out

    def _fw_info(self) -> dict:
        h, src = self._heap, self._src
        packs = src.styles if src else ()
        state = firmware.update_state(h, src)
        plan = fd.plan(h, src)
        if state == "current" and any(plan.get(k) == "need" for k in fd.PACK_KINDS):
            state = "packs"                    # same image, but a pack differs or is missing
        return {"update": state, "plan": plan, "feedError": self._feed_error, "feedBusy": self._feed_pending,
                "styles": {"state": firmware.style_state(h, src),
                           "active": firmware.clock_style(h) or pv.CLASSIC,
                           "packs": [{"id": p.id, "name": p.name, "light": p.light} for p in packs]},
                "seq": firmware.clock_seq(h), "bundle": self._bundle_info()}

    def _style_arg(self, style: Any) -> str | None:
        """The page's style choice, checked: None (leave it), "classic" or a bundled pack id.
        ValueError for anything else."""
        if style is None:
            return None
        packs = self._src.styles if self._src else ()
        if not isinstance(style, str) or (style != pv.CLASSIC and style not in {p.id for p in packs}):
            raise ValueError("unknown style")
        return style

    def _apply_style(self, style: str) -> None:
        """Show `style` on the clock: send its pack, or clear the pack for the classic screen."""
        if style == pv.CLASSIC:
            view = self._api.clear_style()
        else:
            packs = self._src.styles if self._src else ()                  # _style_arg checked it
            pack = next(p for p in packs if p.id == style)
            view = self._api.install_style(pack.read())
        self._heap = {**self._heap, "style": str(view.get("id") or "")}

    def _app_info(self) -> dict:
        return {"version": __version__, "autostart": autostart.is_enabled(),
                "autostartSupported": autostart.supported(), "hooks": _safe(hooks.is_installed),
                "log": str(paths.LOG_FILE), "clock": self._base, "key": self._key,
                "manual": self._store.load().get("manual_host", ""),
                **({"update": self._updater.state()} if self._updater is not None else {})}

    # ---- reading -----------------------------------------------------------------
    def state(self) -> dict:
        """Everything the page shows: heap (no login), config, firmware and app facts. The
        tab the window was opened on rides on the first answer, clock or not."""
        r = self._state()
        tab, self._tab = self._tab, ""
        return {**r, "tab": tab} if tab else r

    def _state(self) -> dict:
        if self._updater is not None:
            self._updater.check_async()           # the app's own update: needs no clock, never blocks
        self._follow_store()     # the tray's latest find first: waiting out a gone address takes ~60 s
        if not self._base:
            return {**_fail("noclock", f"Chưa có đồng hồ. {TYPE_IP}"), "app": self._app_info()}
        try:
            self._heap = self._api.heap()
        except ClockError as e:
            if isinstance(e, AuthNeeded) or not self._follow_store():
                return {**_err(e), "app": self._app_info()}
            try:                                   # the tray found it elsewhere while we waited: ask there
                self._heap = self._api.heap()
            except ClockError as e2:
                return {**_err(e2), "app": self._app_info()}
        self._kick_feed()                          # in the background: the page polls fw() meanwhile
        try:
            cfg = self._api.config()
        except ClockError as e:
            return {**_err(e), "heap": self._heap, "fw": self._fw_info(), "app": self._app_info()}
        self._cfg = cfg
        self._mirror_screen(cfg)
        return {"ok": True, "heap": self._heap, "config": cfg, "fw": self._fw_info(),
                "net": self._net(), "app": self._app_info()}

    def fw(self) -> dict:
        """The firmware/packs facts alone: the page asks again while `feedBusy` is true, to pick
        up the feed's release when the background check finishes."""
        return {"ok": True, "fw": self._fw_info()}

    def _net(self) -> dict | None:
        """The network the clock is on now ({ssid, rssi}), for the WiFi tab's "in use" mark.
        Only decoration: None when the clock does not say (AP mode, an error)."""
        try:
            st = self._api.status()
        except ClockError:
            return None
        ssid = st.get("ssid")
        if st.get("mode") != "sta" or not st.get("connected") or not isinstance(ssid, str) or not ssid:
            return None
        rssi = st.get("rssi")
        return {"ssid": ssid, "rssi": rssi if isinstance(rssi, int) and not isinstance(rssi, bool) else None}

    def _mirror_screen(self, cfg: dict) -> None:
        """The engine stops pushing animations while the main screen is Usage; it
        runs in another process, so hand it the value through the prefs file."""
        s = (cfg.get("usage") or {}).get("screen")
        if isinstance(s, int) and not isinstance(s, bool):
            with contextlib.suppress(OSError):
                self._remember_screen(s)

    def login(self, password: str, user: str = creds.DEFAULT_USER) -> dict:
        if not isinstance(password, str) or not password or not isinstance(user, str) or not user:
            return _fail("bad", "Nhập mật khẩu in trên thẻ đi kèm đồng hồ.")
        self._api.set_auth((user, password))
        try:
            self._api.config()
        except ClockError as e:
            self._api.set_auth(self._keychain.get(self._key))
            return _err(e) if not isinstance(e, AuthNeeded) else _fail("auth", "Sai mật khẩu.")
        saved = self._keychain.put(self._key, user, password)
        return {**self.state(), "saved": saved}

    def usage_preview(self, styles: Any = None, motion: Any = True) -> dict:
        """The clock's usage screen in each style ("classic" or a pack id), with the usage
        the app last sent it.

        {"ok": true, "exact": true, "frames": {"classic": "data:image/png;..."}, "sentAt": <epoch>|null}
        when the clock's own renderer is built into the app: with `motion` each frame is one
        loop of the screen as it animates (an animated PNG), without it (reduced motion) one
        still BMP. With `exact` false there are no frames and the page draws its approximation
        from `payload` (None: no data yet).
        """
        motion = motion if isinstance(motion, bool) else True     # the page is untrusted input
        known = [pv.CLASSIC] + [p.id for p in (self._src.styles if self._src else ())]
        if not isinstance(styles, list) or not styles or not all(isinstance(i, str) and i in known for i in styles):
            styles = known
        stored = self._payload_source()
        payload, sent_at = stored if stored else (None, None)
        now = self._now()
        live = pv.live_payload(payload, now)
        out: dict = {"ok": True, "exact": False, "payload": live, "sentAt": sent_at, "now": now}
        r, cache = self._renderer, self._frames   # once: the feed thread may swap both meanwhile
        if r is None or cache is not self._frames:
            return out
        frames: dict[str, str] = {}
        for i in dict.fromkeys(styles):
            if i != pv.CLASSIC and i not in r.styles:
                continue
            key = pv.frame_key(i, live, int(now // 60), "clip" if motion else "still")
            url = cache.get(key)
            if url is None:
                try:
                    url = pv.apng_data_url(r.clip(i, live)) if motion else pv.bmp_data_url(r.draw(i, live))
                except (OSError, ValueError, subprocess.SubprocessError) as e:
                    print(f"[settings] preview renderer failed: {e}", flush=True)
                    return out                 # the page falls back to its canvas
                cache.put(key, url)
            frames[i] = url
        out.update(exact=True, frames=frames)
        return out

    def clawd_pak(self) -> dict:
        """The clock's Clawd sprites (/clawd.pak) for the page's approximate preview, which
        has no renderer: {"ok": true, "pak": "<base64>"}. Kept in the config folder and read
        again only when /api/heap reports a different pack; never part of the app itself.
        {"ok": false} when there is none to be had (the page then draws the app icon)."""
        cache = pv.PakCache(paths.PAK_FILE)
        have = cache.load()
        want = self._heap.get("pak") if isinstance(self._heap, dict) else None
        want = want if isinstance(want, int) and not isinstance(want, bool) and want > 0 else 0
        if have and (not want or have[1] == want):
            return {"ok": True, "pak": b64encode(have[0]).decode()}
        if not want or not self._base or self._now() - self._pak_failed < PAK_RETRY_S:
            return _fail("nopak", "Đồng hồ chưa có hình Clawd.") if not have else \
                {"ok": True, "pak": b64encode(have[0]).decode()}
        try:
            data = self._api.get_file("/clawd.pak", pv.PAK_MAX)
            pv.pak_count(data)
            with contextlib.suppress(OSError):
                cache.save(data, want)
        except (ClockError, ValueError) as e:
            self._pak_failed = self._now()
            print(f"[settings] could not read clawd.pak from the clock: {e}", flush=True)
            return {"ok": True, "pak": b64encode(have[0]).decode()} if have else _err(e)
        return {"ok": True, "pak": b64encode(data).decode()}

    # ---- saving --------------------------------------------------------------------
    def _diff(self, edited: Any) -> dict:
        return config_diff(_clock_view(self._cfg), clean(edited))

    def preview(self, edited: Any) -> dict:
        """What Save would send: the changed keys, for the save bar."""
        try:
            return {"ok": True, "changes": self._diff(edited)}
        except ValueError as e:
            return _fail("bad", f"Giá trị không hợp lệ ({e}).")

    def save(self, edited: Any, style: Any = None) -> dict:
        """Show `style` ("classic" or a pack id; None: leave it) when it is not what the
        clock shows already, then POST the changed settings. The pack goes first: it is
        kept on the clock's filesystem, so a reboot for a WiFi change keeps it."""
        if self._job["running"]:
            return _fail("busy", "Đang cập nhật firmware, chờ xong đã.")
        try:
            changes = self._diff(edited)
            style = self._style_arg(style)
        except ValueError as e:
            return _fail("bad", f"Giá trị không hợp lệ ({e}).")
        if style == (firmware.clock_style(self._heap) or pv.CLASSIC):
            style = None
        # Back to the classic screen only deletes the clock's pack: it needs neither a bundle nor a
        # feed (a clock that runs a pack already draws the style API; one that does not shows classic).
        if style not in (None, pv.CLASSIC) and firmware.style_state(self._heap, self._src) != "ready":
            return _fail("bad", "Firmware trên đồng hồ chưa vẽ được kiểu này: cập nhật firmware trước.")
        try:
            if style is not None:
                self._apply_style(style)
            r = self._api.save(changes)
        except ClockError as e:
            return _err(e)
        except (OSError, ValueError) as e:
            return _fail("error", f"Gói kiểu màn hình đi kèm app bị hỏng ({e}).")
        if not r.get("reboot"):
            with contextlib.suppress(ClockError):
                self._cfg = self._api.config()
        screen = changes.get("usage", {}).get("screen")
        if screen is not None:
            self._mirror_screen({"usage": {"screen": screen}})
        return {"ok": True, "sent": changes, "reboot": bool(r.get("reboot")), "config": self._cfg,
                "styleSent": style is not None, "fw": self._fw_info()}

    # ---- WiFi ------------------------------------------------------------------------
    def scan(self) -> dict:
        try:
            nets = self._api.scan()
        except ClockError as e:
            return _err(e)
        best: dict[str, dict] = {}
        for n in nets:
            s = n.get("ssid")
            if isinstance(s, str) and s and (s not in best or n.get("rssi", -999) > best[s].get("rssi", -999)):
                best[s] = {"ssid": s, "rssi": n.get("rssi", -100), "enc": bool(n.get("enc"))}
        return {"ok": True, "networks": sorted(best.values(), key=lambda n: -n["rssi"])}

    # ---- maintenance -------------------------------------------------------------------
    def reboot(self) -> dict:
        try:
            self._api.reboot()
        except ClockError as e:
            return _err(e)
        return {"ok": True}

    def factory_reset(self, confirm: str) -> dict:
        if confirm != "XOA":
            return _fail("bad", "Gõ XOA để xác nhận.")
        try:
            self._api.factory_reset()
        except ClockError as e:
            return _err(e)
        return {"ok": True}

    def _dialog(self, save: bool, name: str = "") -> str | None:
        if self._window is None:
            return None
        import webview
        kind = webview.FileDialog.SAVE if save else webview.FileDialog.OPEN
        kw = {"save_filename": name} if save else {"file_types": ("Bản sao lưu (*.json)",)}
        r = self._window.create_file_dialog(kind, **kw)
        if not r:
            return None
        return r if isinstance(r, str) else r[0]

    def export_backup(self) -> dict:
        try:
            data = self._api.export()
        except ClockError as e:
            return _err(e)
        name = f"{self._cfg.get('hostname') or 'clawd'}-{time.strftime('%Y%m%d')}.json"
        path = self._dialog(True, name)
        if not path:
            return {"ok": False, "code": "cancel", "error": ""}
        Path(path).write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
        return {"ok": True, "path": path}

    def import_backup(self) -> dict:
        path = self._dialog(False)
        if not path:
            return {"ok": False, "code": "cancel", "error": ""}
        try:
            data = json.loads(Path(path).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return _fail("bad", "File này không phải bản sao lưu của đồng hồ.")
        if not isinstance(data, dict) or "hostname" not in data:
            return _fail("bad", "File này không phải bản sao lưu của đồng hồ.")
        try:
            self._api.restore(data)
        except ClockError as e:
            return _err(e)
        return {"ok": True}

    # ---- firmware update (runs in the background; the page polls job()) ----------------
    def update(self, edited: Any = None, style: Any = None) -> dict:
        """Install the newest firmware and packs (the feed's release, else the bundled
        firmware) in the background, then save `edited` and show `style` (None: leave both)."""
        if self._src is None:
            return _fail("bad", "Chưa có bản firmware nào để cài (app không kèm firmware, "
                                "và chưa đọc được nguồn cập nhật).")
        try:
            changes = self._diff(edited) if edited is not None else {}
            style = self._style_arg(style)
        except ValueError as e:
            return _fail("bad", f"Giá trị không hợp lệ ({e}).")
        with self._lock:
            if self._job["running"]:
                return _fail("busy", "Đang cập nhật rồi.")
            self._job = {"running": True, "step": "check", "error": "", "done": False, "started": time.time()}
        threading.Thread(target=self._run_update, args=(changes, style), name="clawd-ota", daemon=True).start()
        return {"ok": True}

    def _run_update(self, changes: dict, style: str | None) -> None:
        beat = [0.0]

        def step(s: str) -> None:
            self._job["step"] = s
            if time.monotonic() - beat[0] > 5:      # the tray must not restart the app now
                beat[0] = time.monotonic()
                paths.set_ota_busy(True)
        paths.set_ota_busy(True)
        src = self._src                        # update() only starts this thread with one
        try:
            if src is None:
                raise ValueError("no firmware to install")
            need = src.style_api if style else 0
            if isinstance(src, fd.Release) and self._feed is not None:
                step("download")
                src = self._feed.fetch(src, set(fd.needed(self._api.heap(), src)) | fd.STYLE_KINDS,
                                       progress=lambda *_a: step("download"))
                self._release = src
                heap = ota.install_release(self._api, src, changes, progress=step, need_style_api=need)
            else:
                heap = ota.install_then_save(self._api, src, changes, progress=step, need_style_api=need)
            self._heap = heap
            if style and style != (firmware.clock_style(heap) or pv.CLASSIC):
                self._apply_style(style)
            else:
                self._refresh_style(src)
            with contextlib.suppress(ClockError):
                self._cfg = self._api.config()
            self._mirror_screen(self._cfg)
            self._job.update(done=True, step="done")
        except (ClockError, fd.FeedError, ValueError, OSError) as e:
            self._job.update(error=_err(e)["error"] if isinstance(e, ClockError) else f"Lỗi: {e}")
        finally:
            self._job["running"] = False
            paths.set_ota_busy(False)

    def _refresh_style(self, src: firmware.Bundle) -> None:
        """The clock's style pack, re-sent when the release carries a different build of it."""
        sid = firmware.clock_style(self._heap)
        pack = next((p for p in src.styles if p.id == sid), None) if sid else None
        if pack is None:
            return
        with contextlib.suppress(ClockError, OSError, ValueError):
            if str(self._api.style().get("crc") or "") != f"{pack.crc:08x}":
                self._job["step"] = "style"
                self._apply_style(sid)

    def check_update(self) -> dict:
        """Ask the feed now (the Update tab's button); the clock's heap is read again too."""
        if self._job["running"]:
            return _fail("busy", "Đang cập nhật rồi.")
        with contextlib.suppress(ClockError):
            self._heap = self._api.heap()
        self._check_feed(force=True)
        return {"ok": True, "fw": self._fw_info(), "heap": self._heap, "feed": self._feed is not None}

    def job(self) -> dict:
        return {"ok": True, **{k: v for k, v in self._job.items() if k != "started"}}

    # ---- the app's own update (appupdate.py; the tray downloads too, they share one folder) ---
    def app_update(self) -> dict:
        """The state of the app's update, for the page's polling."""
        if self._updater is None:
            return _fail("bad", "Tự cập nhật app chưa bật.")
        return {"ok": True, **self._updater.state()}

    def app_update_check(self) -> dict:
        """The Update tab's \"Kiểm tra bản mới\" for the app: ask GitHub now, download in the background."""
        if self._updater is None:
            return _fail("bad", "Tự cập nhật app chưa bật.")
        self._updater.check_async(force=True)
        return {"ok": True, **self._updater.state(), "status": "checking"}

    def app_update_apply(self) -> dict:
        """Restart into the downloaded version. The tray owns the process that has to quit, so
        it does the swap; a firmware install in progress is never interrupted."""
        if self._updater is None:
            return _fail("bad", "Tự cập nhật app chưa bật.")
        if self._job["running"]:
            return _fail("busy", "Đang cập nhật firmware cho đồng hồ; hãy đợi xong rồi khởi động lại.")
        st = self._updater.state()
        if st["status"] != "ready":
            return _fail("bad", "Chưa có bản nào được tải xong.")
        if not st["canApply"]:
            return _fail("bad", st["why"] or "Bản này không tự cập nhật được.")
        if not self._notify({"cmd": "app_update_apply"}):
            return _fail("bad", "Mở lại Clawd từ biểu tượng trên thanh menu / khay hệ thống rồi thử lại.")
        return {"ok": True}

    def app_update_page(self) -> dict:
        """Open the release page (an install that cannot replace itself)."""
        st = self._updater.state() if self._updater is not None else {}
        page = st.get("page") or f"https://github.com/{appupdate.REPO}/releases/latest"
        webbrowser.open(page)
        return {"ok": True}

    def busy(self) -> bool:
        return bool(self._job["running"])

    # ---- the app ------------------------------------------------------------------------
    def set_autostart(self, on: bool) -> dict:
        try:
            autostart.set_enabled(bool(on))
        except OSError as e:
            return _fail("error", f"Không đổi được: {e}")
        self._notify({"cmd": "refresh"})
        return {"ok": True, "app": self._app_info()}

    def set_hooks(self, on: bool) -> dict:
        hooks.apply(bool(on))
        self._notify({"cmd": "setup"})
        if on and not hooks.is_installed():
            return _fail("error", "Máy này chưa có Claude Code (thư mục ~/.claude).")
        return {"ok": True, "app": self._app_info()}

    def open_log(self) -> dict:
        from clawd_desktop.tray import open_path
        open_path(str(paths.LOG_FILE))
        return {"ok": True}

    def open_web(self) -> dict:
        webbrowser.open(self._base + "/")
        return {"ok": True}

    def set_clock(self, raw: str) -> dict:
        """Use the clock at this address (typed by hand), or "" for "Tìm lại": the saved address
        again (the tray unpins a typed one so mDNS may replace it), or where mDNS finds the clock
        meanwhile. Only that one address is ever asked: there is no network scan."""
        if not isinstance(raw, str):
            return _fail("bad", "Địa chỉ không hợp lệ.")
        raw = raw.strip()
        c = manual_clock(raw) if raw else None
        if raw and c is None:
            return _fail("bad", f"“{raw}” không phải địa chỉ hợp lệ.")
        self._notify({"cmd": "set_clock", "host": raw})
        if c is not None:
            self._connect(c.base, raw)
            return self.state()
        deadline = time.monotonic() + SEARCH_WAIT
        while True:
            c = stored_clock(self._store.load())     # re-read: the tray may move it (mDNS)
            if c is not None and answers(c.base):
                self._connect(c.base, c.name or c.host)
                return self.state()
            if time.monotonic() >= deadline:
                break
            time.sleep(1.0)
        where = f"Đồng hồ ở {c.host} không trả lời." if c is not None else "Chưa có địa chỉ đồng hồ."
        return _fail("offline", f"{where} {TYPE_IP}")


    # ---- coding agents (the Agent tab and the wizard's Agent step) ----------------------
    def _reg(self) -> dict:
        return self._agents if self._agents is not None else agents.registry()

    def agents(self) -> dict:
        """Every agent Clawd knows, whether it is on this machine and connected. The ones
        shown count as offered, so the tray stops offering them."""
        infos = agents.infos(self._reg())
        firstrun.mark_seen(i["id"] for i in infos if i["found"])
        return {"ok": True, "agents": infos, "wsl": _has_wsl()}

    def set_agent(self, aid: str, on: bool) -> dict:
        a = self._reg().get(str(aid))
        if a is None:
            return _fail("bad", "Không biết agent này.")
        try:
            msg = a.install() if on else a.uninstall()
        except (agents.AgentError, OSError) as e:
            return {**_fail("error", str(e)), "agents": agents.infos(self._reg())}
        self._notify({"cmd": "setup"})
        return {"ok": True, "msg": msg, "agents": agents.infos(self._reg())}

    def set_agents(self, ids: Any) -> dict:
        """The wizard's Agent step: connect the ticked agents, disconnect the unticked ones
        that are connected; nothing else is touched. Errors are listed, the rest still go."""
        want = {str(i) for i in ids} if isinstance(ids, list) else set()
        errors = []
        for aid, a in self._reg().items():
            try:
                on = _safe(a.is_installed)
                if aid in want and not on:
                    a.install()
                elif aid not in want and on:
                    a.uninstall()
            except (agents.AgentError, OSError) as e:
                errors.append(f"{a.name}: {e}")
        self._notify({"cmd": "setup"})
        out = {"ok": not errors, "agents": agents.infos(self._reg())}
        return {**out, "error": "\n".join(errors)} if errors else out

    def disconnect_all(self) -> dict:
        lines = agents.uninstall_all(self._reg())
        self._notify({"cmd": "setup"})
        return {"ok": True, "msg": "\n".join(lines), "agents": agents.infos(self._reg())}

    # ---- the usage source ----------------------------------------------------------------
    def usage_sources(self) -> dict:
        """What the clock's usage screen can show, and the current choice (usage.py)."""
        from clawd_desktop import codex_usage, usage
        dirs = [d for d in usage.read_config_dirs() if d.is_dir()]
        cliproxy = usage.read_onoff_setting("cliproxy") == "on"
        opts = []
        if len(dirs) > 1 or cliproxy:
            opts.append({"id": "claude", "label": "Claude: mọi tài khoản",
                         "detail": "Tự hiện tài khoản đang dùng nhiều nhất.", "ok": True})
            opts += [{"id": f"claude:{d}", "label": f"Claude: {d.name}", "detail": str(d), "ok": True}
                     for d in dirs]
        else:
            opts.append({"id": "claude", "label": "Claude Code", "ok": bool(dirs),
                         "detail": str(dirs[0]) if dirs else "Chưa thấy Claude Code trên máy này."})
        codex_ok = codex_usage.available()
        codex_now = codex_usage.read() if codex_ok else None
        opts.append({"id": "codex", "label": "Codex CLI", "ok": codex_ok,
                     "detail": ("Theo lượt Codex gần nhất trên máy này." if codex_now else
                                "Gói Codex này chưa có giới hạn 5 giờ / tuần để hiện." if codex_ok else
                                "Chưa thấy Codex CLI trên máy này.")})
        opts.append({"id": "off", "label": "Không hiện usage", "ok": True,
                     "detail": "Đồng hồ chỉ hiện giờ và Clawd."})
        return {"ok": True, "options": opts, "current": usage.read_usage_source()}

    def set_usage_source(self, value: str) -> dict:
        from clawd_desktop import usage
        ids = {o["id"] for o in self.usage_sources()["options"]}
        if value not in ids:
            return _fail("bad", "Nguồn usage không hợp lệ.")
        prefs = paths.load_prefs()
        if value == "claude":
            prefs.pop("usage_source", None)        # the default: also what older versions did
        else:
            prefs["usage_source"] = value
        paths.save_prefs(prefs)
        self._notify({"cmd": "usage_source"})
        return {"ok": True, "current": usage.read_usage_source()}

    # ---- the setup wizard ------------------------------------------------------------------
    def setup_state(self) -> dict:
        prefs = firstrun.load()
        left = firstrun.pending(prefs)
        return {"ok": True, "steps": list(firstrun.STEPS), "pending": left,
                "fresh": "clock" in left, "app": self._app_info()}

    def finish_setup(self, steps: Any) -> dict:
        """Steps done or skipped (each step saves its own choice first)."""
        firstrun.mark_done(str(s) for s in steps) if isinstance(steps, list) else None
        self._notify({"cmd": "setup"})
        return self.setup_state()

    def setup_clocks(self) -> dict:
        """The wizard's clock step, asked every ~2 s: clocks mDNS sees, and whether the one
        the app uses now answers. mDNS only: nothing on the network is scanned."""
        if self._mdns is None:
            self._mdns = self._mdns_factory()
            if not self._mdns.start():
                log("mDNS unavailable in the settings window")
        names = self._mdns.names()
        for n in names:
            if n not in self._mdns_seen:
                c = self._mdns.resolve(n)
                if c is not None:
                    self._mdns_seen[n] = c
        for n in list(self._mdns_seen):
            if n not in names:
                del self._mdns_seen[n]
        self._follow_store()
        cur = stored_clock(self._store.load())
        current = None
        if cur is not None:
            current = {**_clock_dict(cur), "answers": answers(cur.base)}
        return {"ok": True, "clocks": [_clock_dict(c) for c in self._mdns_seen.values()],
                "current": current}

    def setup_use_clock(self, name: str) -> dict:
        """Use the clock mDNS found under this name (remembered by name)."""
        c = next((c for c in self._mdns_seen.values() if c.name == name), None)
        if c is None:
            return _fail("gone", "Không còn thấy đồng hồ này. Thử lại hoặc nhập IP.")
        self._notify({"cmd": "use_clock", "name": c.name, "host": c.host, "port": c.port,
                      "version": c.version})
        self._connect(c.base, c.name)
        return {"ok": True, "current": {**_clock_dict(c), "answers": answers(c.base)}}

    def close_mdns(self) -> None:
        if self._mdns is not None:
            self._mdns.stop()
            self._mdns = None


def _clock_dict(c: Clock) -> dict:
    return {"name": c.name, "host": c.host, "port": c.port, "version": c.version, "base": c.base}


def _safe(fn: Callable[[], bool]) -> bool:
    try:
        return bool(fn())
    except Exception:            # a config that does not parse: not installed, as far as we can tell
        return False


def _has_wsl() -> bool:
    import shutil
    import sys
    return sys.platform == "win32" and shutil.which("wsl.exe") is not None


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] [settings] {msg}", flush=True)


def answers(base: str) -> bool:
    """One quick GET /api/heap (no login needed; 401 still means a clock is there)."""
    try:
        r = httpx.get(base + "/api/heap", timeout=PROBE_TIMEOUT)
    except httpx.HTTPError:
        return False
    return r.status_code in (200, 401)

def initial_edit(cfg: dict) -> dict:
    """The page's starting copy of the editable settings."""
    return copy.deepcopy(_clock_view(cfg))
