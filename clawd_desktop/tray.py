"""The tray icon (menu bar on macOS, notification area on Windows), in Vietnamese.

pystray owns the main thread (AppKit requires it); the Engine runs in its own
thread. Menu texts are recomputed every few seconds from Engine.snapshot().
"""
from __future__ import annotations

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
        self.hooks_on = hooks.is_installed()
        self.auto_on = autostart.is_enabled()
        self._icons = {s: icons.status_icon(s) for s in ("ok", "busy", "error")}
        M = pystray.MenuItem
        self.icon = pystray.Icon(
            "clawd", self._icons["busy"], APP_NAME,
            menu=pystray.Menu(
                M(lambda _i: self.head, None, enabled=False),
                M(lambda _i: self.detail, None, enabled=False, visible=lambda _i: bool(self.detail)),
                pystray.Menu.SEPARATOR,
                M("Mở cùng máy tính", self.toggle_autostart, checked=lambda _i: self.auto_on,
                  visible=lambda _i: autostart.supported()),
                M("Clawd phản ứng khi dùng Claude Code", self.toggle_hooks,
                  checked=lambda _i: self.hooks_on),
                pystray.Menu.SEPARATOR,
                M("Mở cài đặt Mạng cục bộ…", lambda: open_path(LOCAL_NETWORK_PANE),
                  visible=lambda _i: sys.platform == "darwin"
                  and bool(self.engine.snapshot().get("lan_blocked"))),
                M("Tìm lại đồng hồ", self.rescan),
                M("Nhập địa chỉ đồng hồ…", self.manual),
                M("Mở trang cài đặt đồng hồ", self.open_clock,
                  visible=lambda _i: self.engine.snapshot().get("clock") is not None),
                M("Mở nhật ký", lambda: open_path(str(paths.LOG_FILE))),
                pystray.Menu.SEPARATOR,
                M(f"Thoát {APP_NAME} (v{__version__})", self.quit),
            ))
        self._stop = threading.Event()

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
    def toggle_autostart(self) -> None:
        self.auto_on = not self.auto_on
        try:
            autostart.set_enabled(self.auto_on)
        except OSError as e:
            log(f"Autostart change failed: {e}")
            self.auto_on = autostart.is_enabled()
        self.on_main(self.icon.update_menu)

    def toggle_hooks(self) -> None:
        want = not self.hooks_on
        msgs = hooks.apply(want)
        self.hooks_on = hooks.is_installed()
        if want and not self.hooks_on:
            alert("Chưa cài được: máy này chưa có Claude Code (thư mục ~/.claude)."
                  + ("\n" + "\n".join(msgs) if msgs else ""))
        self.on_main(self.icon.update_menu)

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

    def open_clock(self) -> None:
        clock = self.engine.snapshot().get("clock")
        if clock is not None:
            webbrowser.open(clock.base + "/")

    def quit(self) -> None:
        self._stop.set()
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
