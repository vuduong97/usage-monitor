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
from pathlib import Path
from typing import Any, Callable

import clawd_desktop.preview as pv
from clawd_desktop import __version__, autostart, creds, firmware, hooks, ota, paths
from clawd_desktop.clockapi import AuthNeeded, ClockApi, ClockError, config_diff
from clawd_desktop.discovery import ClockStore, manual_clock

PAK_RETRY_S = 300       # after a failed /clawd.pak read, wait this long before asking again
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
                 notify: Callable[[dict], None] = lambda _m: None,
                 api_factory: Callable[..., ClockApi] = ClockApi,
                 bundle: firmware.Bundle | None = None,
                 store: ClockStore | None = None,
                 keychain=creds,
                 remember_screen: Callable[[int], None] = paths.set_clock_screen,
                 renderer: pv.Renderer | None = None,
                 payload_source: Callable[[], tuple[dict, float] | None] = paths.usage_payload,
                 now: Callable[[], float] = time.time) -> None:
        self._renderer = renderer              # draws the clock's screen (None: the page approximates)
        self._payload_source = payload_source  # the last usage payload the engine sent
        self._now = now
        self._frames = pv.FrameCache()
        self._pak_failed = 0.0                 # when reading /clawd.pak off the clock last failed
        self._remember_screen = remember_screen  # tell the engine the main screen
        self._notify = notify                  # tell the tray (rescan, set host, refresh)
        self._factory = api_factory
        self._bundle = bundle
        self._store = store or ClockStore(paths.CLOCK_FILE)
        self._keychain = keychain
        self._window = None                    # set by settings_app, for file dialogs
        self._lock = threading.Lock()
        self._job: dict = {"running": False}
        self._heap: dict = {}
        self._cfg: dict = {}
        self._connect(base, key)

    # ---- plumbing --------------------------------------------------------------
    def _connect(self, base: str, key: str) -> None:
        self._base, self._key = base.rstrip("/"), key or base
        self._api = self._factory(self._base, auth=self._keychain.get(self._key) if self._key else None)

    def _follow_store(self) -> bool:
        """The window gets its clock once, when it opens; the tray keeps looking and saves
        where it finds the clock (another network, a new DHCP lease) in clock.json. Use that
        address when it differs from ours. True if it switched."""
        d = self._store.load()
        typed = str(d.get("manual_host") or "")
        c = manual_clock(typed) if typed else (
            manual_clock(f"{d['host']}:{d.get('port') or 80}") if d.get("host") else None)
        if c is None or c.base.rstrip("/") == self._base:
            return False
        name = str(d.get("name") or "")
        self._connect(c.base, name or self._key)    # the same clock keeps its saved login
        return True

    def _bundle_info(self) -> dict | None:
        b = self._bundle
        return None if b is None else {"seq": b.seq, "size": b.size, "commit": b.commit, "styleApi": b.style_api}

    def _fw_info(self) -> dict:
        h = self._heap
        packs = self._bundle.styles if self._bundle else ()
        return {"update": firmware.update_state(h, self._bundle),
                "styles": {"state": firmware.style_state(h, self._bundle),
                           "active": firmware.clock_style(h) or pv.CLASSIC,
                           "packs": [{"id": p.id, "name": p.name, "light": p.light} for p in packs]},
                "seq": firmware.clock_seq(h), "bundle": self._bundle_info()}

    def _style_arg(self, style: Any) -> str | None:
        """The page's style choice, checked: None (leave it), "classic" or a bundled pack id.
        ValueError for anything else."""
        if style is None:
            return None
        packs = self._bundle.styles if self._bundle else ()
        if not isinstance(style, str) or (style != pv.CLASSIC and style not in {p.id for p in packs}):
            raise ValueError("unknown style")
        return style

    def _apply_style(self, style: str) -> None:
        """Show `style` on the clock: send its pack, or clear the pack for the classic screen."""
        if style == pv.CLASSIC:
            view = self._api.clear_style()
        else:
            pack = next(p for p in self._bundle.styles if p.id == style)   # _style_arg checked it
            view = self._api.install_style(pack.read())
        self._heap = {**self._heap, "style": str(view.get("id") or "")}

    def _app_info(self) -> dict:
        return {"version": __version__, "autostart": autostart.is_enabled(),
                "autostartSupported": autostart.supported(), "hooks": hooks.is_installed(),
                "log": str(paths.LOG_FILE), "clock": self._base, "key": self._key,
                "manual": self._store.load().get("manual_host", "")}

    # ---- reading -----------------------------------------------------------------
    def state(self) -> dict:
        """Everything the page shows: heap (no login), config, firmware and app facts."""
        self._follow_store()     # the tray's latest find first: waiting out a gone address takes ~60 s
        if not self._base:
            return {**_fail("noclock", "Chưa tìm thấy đồng hồ."), "app": self._app_info()}
        try:
            self._heap = self._api.heap()
        except ClockError as e:
            if isinstance(e, AuthNeeded) or not self._follow_store():
                return {**_err(e), "app": self._app_info()}
            try:                                   # the tray found it elsewhere while we waited: ask there
                self._heap = self._api.heap()
            except ClockError as e2:
                return {**_err(e2), "app": self._app_info()}
        try:
            cfg = self._api.config()
        except ClockError as e:
            return {**_err(e), "heap": self._heap, "fw": self._fw_info(), "app": self._app_info()}
        self._cfg = cfg
        self._mirror_screen(cfg)
        return {"ok": True, "heap": self._heap, "config": cfg, "fw": self._fw_info(),
                "app": self._app_info()}

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
        known = [pv.CLASSIC] + [p.id for p in (self._bundle.styles if self._bundle else ())]
        if not isinstance(styles, list) or not styles or not all(isinstance(i, str) and i in known for i in styles):
            styles = known
        stored = self._payload_source()
        payload, sent_at = stored if stored else (None, None)
        now = self._now()
        live = pv.live_payload(payload, now)
        out: dict = {"ok": True, "exact": False, "payload": live, "sentAt": sent_at, "now": now}
        r = self._renderer
        if r is None:
            return out
        frames: dict[str, str] = {}
        for i in dict.fromkeys(styles):
            if i != pv.CLASSIC and i not in r.styles:
                continue
            key = pv.frame_key(i, live, int(now // 60), "clip" if motion else "still")
            url = self._frames.get(key)
            if url is None:
                try:
                    url = pv.apng_data_url(r.clip(i, live)) if motion else pv.bmp_data_url(r.draw(i, live))
                except (OSError, ValueError, subprocess.SubprocessError) as e:
                    print(f"[settings] preview renderer failed: {e}", flush=True)
                    return out                 # the page falls back to its canvas
                self._frames.put(key, url)
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
        if style is not None and firmware.style_state(self._heap, self._bundle) != "ready":
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
        """Install the bundled firmware in the background, then save `edited` and show
        `style` (None: leave both)."""
        if self._bundle is None:
            return _fail("bad", "Bản app này không kèm firmware.")
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
        def step(s: str) -> None:
            self._job["step"] = s
        bundle = self._bundle                  # update() only starts this thread with one
        try:
            if bundle is None:
                raise ValueError("no bundled firmware")
            heap = ota.install_then_save(self._api, bundle, changes, progress=step,
                                         need_style_api=bundle.style_api if style else 0)
            self._heap = heap
            if style and style != (firmware.clock_style(heap) or pv.CLASSIC):
                self._apply_style(style)
            with contextlib.suppress(ClockError):
                self._cfg = self._api.config()
            self._mirror_screen(self._cfg)
            self._job.update(done=True)
        except (ClockError, ValueError, OSError) as e:
            self._job.update(error=_err(e)["error"] if isinstance(e, ClockError) else f"Lỗi: {e}")
        finally:
            self._job["running"] = False

    def job(self) -> dict:
        return {"ok": True, **{k: v for k, v in self._job.items() if k != "started"}}

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
        self._notify({"cmd": "refresh"})
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
        """Use the clock at this address (typed by hand), or "" to search again."""
        if not isinstance(raw, str):
            return _fail("bad", "Địa chỉ không hợp lệ.")
        raw = raw.strip()
        c = manual_clock(raw) if raw else None
        if raw and c is None:
            return _fail("bad", f"“{raw}” không phải địa chỉ hợp lệ.")
        t0 = time.time_ns()
        self._notify({"cmd": "set_clock", "host": raw})
        if c is not None:
            self._connect(c.base, raw)
            return self.state()
        deadline = time.monotonic() + 45          # wait for the tray to find one and save it
        while time.monotonic() < deadline:
            time.sleep(1.0)
            try:
                fresh = self._store.path.stat().st_mtime_ns >= t0
            except OSError:
                fresh = False
            d = self._store.load()
            if fresh and d.get("host") and not d.get("manual_host"):
                port = int(d.get("port") or 80)
                base = f"http://{d['host']}" + (f":{port}" if port != 80 else "")
                self._connect(base, d.get("name") or d["host"])
                return self.state()
        return _fail("offline", "Chưa tìm thấy đồng hồ nào trong mạng.")


def initial_edit(cfg: dict) -> dict:
    """The page's starting copy of the editable settings."""
    return copy.deepcopy(_clock_view(cfg))
