"""The tray icon (menu bar on macOS, notification area on Windows), in Vietnamese.

pystray owns the main thread (AppKit requires it); the Engine runs in its own
thread. Menu texts are recomputed every few seconds from Engine.snapshot().
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import time
import webbrowser

from clawd_desktop import APP_NAME, __version__, autostart, hooks, icons, paths
from clawd_desktop.engine import Engine, daemon_running

REFRESH = 3.0
FRESH = 180.0      # a push this recent counts as "connected"
LOCAL_NETWORK_PANE = "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


# =============================================================================
# Status text (pure, tested)
# =============================================================================
def status(snap: dict, now: float | None = None) -> tuple[str, str, str]:
    """(dot state, headline, detail line) for the menu and tooltip."""
    now = time.time() if now is None else now
    clock = snap.get("clock")
    last_ok = snap.get("last_ok")
    connected = bool(last_ok and now - last_ok < FRESH)
    payload = snap.get("payload") or {}
    detail = ""
    if payload.get("ok") is not False and ("s" in payload or "w" in payload) and snap.get("payload_at"):
        at = time.strftime("%H:%M", time.localtime(snap["payload_at"]))
        detail = f"Cập nhật {at} · Phiên {payload.get('s', 0)}% · Tuần {payload.get('w', 0)}%"

    if snap.get("token") is False:
        return "error", "Chưa đăng nhập Claude: mở Terminal, gõ claude login", detail
    if snap.get("manual_config"):
        where = ", ".join(snap.get("config_targets") or [])
        return ("ok" if connected else "busy",
                ("Đã kết nối: " if connected else "Đang gửi tới: ") + where, detail)
    if clock is None:
        if snap.get("lan_blocked"):
            if sys.platform == "darwin":
                return "error", "macOS đang chặn Clawd: bật Clawd trong Cài đặt → Mạng cục bộ", detail
            return "error", "Máy tính chặn Clawd vào mạng: cho phép Clawd trong Tường lửa", detail
        if snap.get("scanning"):
            return "busy", "Đang dò tìm đồng hồ trong mạng…", detail
        return "error", "Không thấy đồng hồ: cùng WiFi chưa? (mạng công ty/mạng khách có thể chặn)", detail
    if connected:
        return "ok", f"Đã kết nối: {clock.label}", detail
    if snap.get("failing"):
        return "error", f"Đồng hồ {clock.label} không trả lời", detail
    return "busy", f"Đang kết nối {clock.label}…", detail


# =============================================================================
# Small native dialogs (no extra GUI toolkit)
# =============================================================================
def _no_window() -> dict:
    if sys.platform == "win32":
        return {"creationflags": 0x08000000}  # CREATE_NO_WINDOW
    return {}


def ask_text(prompt: str, default: str = "") -> str | None:
    """One-line text input; None when cancelled or unsupported."""
    try:
        if sys.platform == "darwin":
            esc = lambda s: s.replace("\\", "\\\\").replace('"', '\\"')  # noqa: E731
            script = (f'text returned of (display dialog "{esc(prompt)}" '
                      f'default answer "{esc(default)}" with title "{APP_NAME}")')
            r = subprocess.run(["osascript", "-e", script], capture_output=True, text=True, timeout=300)
            return r.stdout.strip() if r.returncode == 0 else None
        if sys.platform == "win32":
            ps = ("Add-Type -AssemblyName Microsoft.VisualBasic; "
                  "[Microsoft.VisualBasic.Interaction]::InputBox($env:CLAWD_PROMPT, 'Clawd', $env:CLAWD_DEFAULT)")
            env = {**os.environ, "CLAWD_PROMPT": prompt, "CLAWD_DEFAULT": default}
            r = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True,
                               text=True, timeout=300, env=env, **_no_window())
            text = r.stdout.strip()
            return text or None
    except (OSError, subprocess.SubprocessError):
        return None
    return None


def alert(text: str) -> None:
    try:
        if sys.platform == "darwin":
            esc = text.replace("\\", "\\\\").replace('"', '\\"')
            subprocess.run(["osascript", "-e", f'display alert "{APP_NAME}" message "{esc}"'],
                           timeout=300)
        elif sys.platform == "win32":
            import ctypes
            ctypes.windll.user32.MessageBoxW(None, text, APP_NAME, 0x40)  # type: ignore[attr-defined]
        else:
            print(text)
    except (OSError, subprocess.SubprocessError):
        pass


def open_path(path: str) -> None:
    try:
        if sys.platform == "darwin":
            subprocess.Popen(["open", path])
        elif sys.platform == "win32":
            os.startfile(path)  # type: ignore[attr-defined]
        else:
            subprocess.Popen(["xdg-open", path])
    except OSError:
        pass


# =============================================================================
# First run
# =============================================================================
def first_run() -> None:
    prefs = paths.load_prefs()
    if prefs.get("first_run_done"):
        return
    if autostart.supported():
        try:
            autostart.enable()
            log("Autostart enabled")
        except OSError as e:
            log(f"Autostart not enabled: {e}")
    try:
        hooks.apply(True)
    except OSError as e:
        log(f"Hooks not installed: {e}")
    prefs["first_run_done"] = True
    paths.save_prefs(prefs)


# =============================================================================
# The tray
# =============================================================================
class Tray:
    def __init__(self, engine: Engine) -> None:
        import pystray  # imported late: a display/session is needed
        self.pystray = pystray
        self.engine = engine
        self.state = ""
        self.head = "Đang khởi động…"
        self.detail = ""
        self._icons = {s: icons.status_icon(s) for s in ("ok", "busy", "error")}
        M = pystray.MenuItem
        self.icon = pystray.Icon(
            "clawd", self._icons["busy"], APP_NAME,
            menu=pystray.Menu(
                M(lambda _i: self.head, None, enabled=False),
                M(lambda _i: self.detail, None, enabled=False, visible=lambda _i: bool(self.detail)),
                pystray.Menu.SEPARATOR,
                M("Cài đặt…", self.open_settings, default=True),
                M("Mở cài đặt Mạng cục bộ…", lambda: open_path(LOCAL_NETWORK_PANE),
                  visible=lambda _i: sys.platform == "darwin"
                  and bool(self.engine.snapshot().get("lan_blocked"))),
                pystray.Menu.SEPARATOR,
                # Fallbacks that work without the settings window (e.g. no WebView2).
                M("Tìm lại đồng hồ", self.rescan),
                M("Nhập địa chỉ đồng hồ…", self.manual),
                M("Mở trang cài đặt đồng hồ", self.open_clock,
                  visible=lambda _i: self.engine.snapshot().get("clock") is not None),
                pystray.Menu.SEPARATOR,
                M(f"Thoát {APP_NAME} (v{__version__})", self.quit),
            ))
        self._stop = threading.Event()
        self._settings: subprocess.Popen | None = None

    # ---- main-thread helper for AppKit -----------------------------------------
    def on_main(self, fn) -> None:
        if sys.platform == "darwin":
            try:
                from PyObjCTools import AppHelper
                AppHelper.callAfter(fn)
                return
            except ImportError:
                pass
        fn()

    def refresh(self) -> None:
        state, head, detail = status(self.engine.snapshot())
        changed = (state, head, detail) != (self.state, self.head, self.detail)
        self.head, self.detail = head, detail
        if state != self.state:
            self.state = state
            self.on_main(lambda: setattr(self.icon, "icon", self._icons[state]))
        if changed:
            title = f"{APP_NAME}: {head}" + (f"\n{detail}" if detail else "")
            self.on_main(lambda: (setattr(self.icon, "title", title[:120]), self.icon.update_menu()))

    def _loop(self) -> None:
        while not self._stop.wait(REFRESH):
            try:
                self.refresh()
            except Exception as e:  # never let a status glitch kill the tray
                log(f"Tray refresh failed: {e!r}")

    # ---- menu actions ------------------------------------------------------------
    def rescan(self) -> None:
        self.engine.finder.forget()
        self.refresh()

    def manual(self) -> None:
        def ask() -> None:
            text = ask_text("Địa chỉ IP của đồng hồ (hiện trên màn hình đồng hồ lúc khởi động), "
                            "ví dụ 192.168.1.48:")
            if text and self.engine.finder.set_manual(text) is None:
                alert(f"“{text}” không phải địa chỉ hợp lệ.")
            self.refresh()
        threading.Thread(target=ask, daemon=True).start()

    def open_settings(self) -> None:
        """The settings window, in its own process (pywebview wants the main
        thread, which the tray holds). A second click brings it forward."""
        from clawd_desktop import settings_app
        p = self._settings
        if p is not None and p.poll() is None:
            try:
                p.stdin.write(b"show\n")
                p.stdin.flush()
                return
            except (OSError, ValueError):
                pass
        clock = self.engine.snapshot().get("clock")
        base = clock.base if clock is not None else ""
        key = (clock.name or self._typed_host() or clock.host) if clock is not None else ""
        try:
            p = subprocess.Popen(settings_app.command(base, key), stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, **_no_window())
        except OSError as e:
            log(f"Settings window did not start: {e}")
            return
        self._settings = p
        threading.Thread(target=self._settings_cmds, args=(p,), name="clawd-settings", daemon=True).start()

    def _typed_host(self) -> str:
        return str(self.engine.finder.store.load().get("manual_host") or "")

    def _settings_cmds(self, p: subprocess.Popen) -> None:
        for line in p.stdout:
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            cmd = msg.get("cmd") if isinstance(msg, dict) else None
            if cmd == "set_clock":
                host = str(msg.get("host") or "")
                if host:
                    self.engine.finder.set_manual(host)
                else:
                    self.engine.finder.forget()
                self.refresh()

    def open_clock(self) -> None:
        clock = self.engine.snapshot().get("clock")
        if clock is not None:
            webbrowser.open(clock.base + "/")

    def quit(self) -> None:
        self._stop.set()
        if self._settings is not None and self._settings.poll() is None:
            self._settings.terminate()
        self.engine.stop()
        self.icon.stop()

    def run(self) -> None:
        def setup(icon) -> None:
            icon.visible = True
            threading.Thread(target=self._loop, name="clawd-tray", daemon=True).start()
            self.refresh()
        self.icon.run(setup=setup)


def main() -> int:
    if paths.frozen() or not sys.stdout or not sys.stdout.isatty():
        paths.open_log()
    log(f"{APP_NAME} {__version__} starting ({sys.platform})")
    if daemon_running():
        log("Another Clawd daemon already owns the event port; exiting")
        alert("Clawd đang chạy rồi (xem biểu tượng trên thanh menu / khay hệ thống).")
        return 0
    first_run()
    engine = Engine()
    engine.start()
    try:
        Tray(engine).run()
    finally:
        engine.stop()
    log("Bye")
    return 0
