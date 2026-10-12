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
from pathlib import Path
from typing import Any, Callable

import clawd_desktop.preview as pv
from clawd_desktop import __version__, autostart, creds, firmware, hooks, ota, paths
from clawd_desktop.clockapi import AuthNeeded, ClockApi, ClockError, config_diff
from clawd_desktop.discovery import ClockStore, manual_clock

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

    def _bundle_info(self) -> dict | None:
        b = self._bundle
        return None if b is None else {"seq": b.seq, "layouts": b.layouts, "size": b.size, "commit": b.commit}

    def _fw_info(self) -> dict:
        h = self._heap
        return {"update": firmware.update_state(h, self._bundle),
                "layouts": [firmware.layout_state(i, h, self._bundle) for i in range(firmware.LAYOUT_COUNT)],
                "seq": firmware.clock_seq(h), "bundle": self._bundle_info()}

    def _app_info(self) -> dict:
        return {"version": __version__, "autostart": autostart.is_enabled(),
                "autostartSupported": autostart.supported(), "hooks": hooks.is_installed(),
                "log": str(paths.LOG_FILE), "clock": self._base, "key": self._key,
                "manual": self._store.load().get("manual_host", "")}

    # ---- reading -----------------------------------------------------------------
    def state(self) -> dict:
        """Everything the page shows: heap (no login), config, firmware and app facts."""
        if not self._base:
            return {**_fail("noclock", "Chưa tìm thấy đồng hồ."), "app": self._app_info()}
        try:
            self._heap = self._api.heap()
        except ClockError as e:
            return {**_err(e), "app": self._app_info()}
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

    def usage_preview(self, layouts: Any = None) -> dict:
        """The clock's usage screen for each layout, with the usage the app last sent it.

        {"ok": true, "exact": true, "frames": {"0": "data:image/bmp;..."}, "sentAt": <epoch>|null}
        when the clock's own renderer is built into the app; with `exact` false there are
        no frames and the page draws its approximation from `payload` (None: no data yet).
        """
        if (not isinstance(layouts, list) or not layouts
                or not all(isinstance(i, int) and not isinstance(i, bool) and 0 <= i < firmware.LAYOUT_COUNT
                           for i in layouts)):
            layouts = list(range(firmware.LAYOUT_COUNT))
        stored = self._payload_source()
        payload, sent_at = stored if stored else (None, None)
        now = self._now()
        live = pv.live_payload(payload, now)
        out: dict = {"ok": True, "exact": False, "payload": live, "sentAt": sent_at, "now": now}
        r = self._renderer
        if r is None:
            return out
        frames: dict[str, str] = {}
        for i in dict.fromkeys(layouts):
            if not r.layouts >> i & 1:
                continue
            key = pv.frame_key(i, live, int(now // 60))
            url = self._frames.get(key)
            if url is None:
                try:
                    url = pv.bmp_data_url(r.draw(i, live))
                except (OSError, ValueError, subprocess.SubprocessError) as e:
                    print(f"[settings] preview renderer failed: {e}", flush=True)
                    return out                 # the page falls back to its canvas
                self._frames.put(key, url)
            frames[str(i)] = url
        out.update(exact=True, frames=frames)
        return out

    # ---- saving --------------------------------------------------------------------
    def _diff(self, edited: Any) -> dict:
        return config_diff(_clock_view(self._cfg), clean(edited))

    def preview(self, edited: Any) -> dict:
        """What Save would send: the changed keys, for the save bar."""
        try:
            return {"ok": True, "changes": self._diff(edited)}
        except ValueError as e:
            return _fail("bad", f"Giá trị không hợp lệ ({e}).")

    def save(self, edited: Any) -> dict:
        if self._job["running"]:
            return _fail("busy", "Đang cập nhật firmware, chờ xong đã.")
        try:
            changes = self._diff(edited)
        except ValueError as e:
            return _fail("bad", f"Giá trị không hợp lệ ({e}).")
        layout = changes.get("usage", {}).get("layout")
        if layout is not None and not firmware.clock_layouts(self._heap) >> layout & 1:
            return _fail("bad", "Firmware trên đồng hồ chưa vẽ được kiểu này: cập nhật firmware trước.")
        try:
            r = self._api.save(changes)
        except ClockError as e:
            return _err(e)
        if not r.get("reboot"):
            with contextlib.suppress(ClockError):
                self._cfg = self._api.config()
        screen = changes.get("usage", {}).get("screen")
        if screen is not None:
            self._mirror_screen({"usage": {"screen": screen}})
        return {"ok": True, "sent": changes, "reboot": bool(r.get("reboot")), "config": self._cfg}

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
    def update(self, edited: Any = None, layout: int | None = None) -> dict:
        if self._bundle is None:
            return _fail("bad", "Bản app này không kèm firmware.")
        if layout is not None and (isinstance(layout, bool) or not isinstance(layout, int)):
            return _fail("bad", "Kiểu màn hình không hợp lệ.")
        try:
            changes = self._diff(edited) if edited is not None else {}
        except ValueError as e:
            return _fail("bad", f"Giá trị không hợp lệ ({e}).")
        with self._lock:
            if self._job["running"]:
                return _fail("busy", "Đang cập nhật rồi.")
            self._job = {"running": True, "step": "check", "error": "", "done": False, "started": time.time()}
        threading.Thread(target=self._run_update, args=(changes, layout), name="clawd-ota", daemon=True).start()
        return {"ok": True}

    def _run_update(self, changes: dict, layout: int | None) -> None:
        def step(s: str) -> None:
            self._job["step"] = s
        bundle = self._bundle                  # update() only starts this thread with one
        try:
            if bundle is None:
                raise ValueError("no bundled firmware")
            heap = ota.install_then_save(self._api, bundle, changes, need_layout=layout, progress=step)
            self._heap = heap
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
