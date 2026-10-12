"""The settings window: `Clawd settings [--clock http://10.0.0.5] [--key clawd]`.

Its own process (pywebview needs the main thread, which pystray already owns
in the tray process), started by the tray's "Cài đặt…" item, so it costs
nothing while closed. The tray talks to it over the child's stdin/stdout:

  tray -> window   "show"                         bring the window forward
  window -> tray   {"cmd": "refresh"}             autostart/hooks changed
                   {"cmd": "set_clock", "host": "10.0.0.5" | ""}   typed clock / search again

Started by hand (no tray) it works the same; set_clock then edits clock.json
directly, which the next tray start picks up.
"""
from __future__ import annotations

import json
import sys
import threading
import time
from pathlib import Path

from clawd_desktop import APP_NAME, firmware, paths
from clawd_desktop.settings_api import SettingsApi

UI_DIR = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent.parent)) / "clawd_desktop" / "ui"
WIDTH, HEIGHT = 820, 600


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] [settings] {msg}", flush=True)


def command(clock: str, key: str) -> list[str]:
    """How the tray starts this window (frozen app: the same binary)."""
    head = [sys.executable] if paths.frozen() else [sys.executable, "-m", "clawd_desktop"]
    return head + ["settings", "--piped", "--clock", clock, "--key", key]


class Channel:
    """The pipe to the tray, or (started by hand) direct edits of clock.json."""

    def __init__(self, out, piped: bool) -> None:
        self.out, self.piped = out, piped
        self._lock = threading.Lock()

    def send(self, msg: dict) -> None:
        if self.piped:
            with self._lock:
                try:
                    self.out.write(json.dumps(msg) + "\n")
                    self.out.flush()
                    return
                except (OSError, ValueError, AttributeError):
                    pass
        if msg.get("cmd") == "set_clock":
            from clawd_desktop.discovery import ClockStore
            store = ClockStore(paths.CLOCK_FILE)
            store.save({"manual_host": msg["host"]} if msg.get("host") else {})


def main(clock: str, key: str, piped: bool) -> int:
    pipe_out = sys.stdout                  # the tray reads commands here; keep it before logging moves stdout
    paths.open_log()
    log(f"window for {clock or 'no clock'}")
    import webview

    chan = Channel(pipe_out, piped)
    api = SettingsApi(clock, key, notify=chan.send, bundle=firmware.load_bundle())
    win = webview.create_window(
        f"{APP_NAME} · Cài đặt", url=str(UI_DIR / "index.html"), js_api=api,
        width=WIDTH, height=HEIGHT, min_size=(720, 540), background_color="#141413",
        text_select=False)
    api._window = win

    def closing() -> bool:
        if api.busy():                      # never leave a firmware upload half done
            win.evaluate_js("window.clawdBusyClose && window.clawdBusyClose()")
            return False
        return True
    win.events.closing += closing

    def listen() -> None:                   # "show" from the tray
        for line in sys.stdin:
            if line.strip() == "show":
                try:
                    win.restore()
                    win.show()
                    _front()
                except Exception as e:      # the window may be closing
                    log(f"show failed: {e!r}")
    if piped:
        threading.Thread(target=listen, name="clawd-settings-in", daemon=True).start()

    webview.start(_front, private_mode=True)
    log("window closed")
    return 0


def _front() -> None:
    """macOS: a menu-bar-only app's window opens behind the frontmost app."""
    if sys.platform != "darwin":
        return
    try:
        from AppKit import NSApplication
        from PyObjCTools import AppHelper
        AppHelper.callAfter(lambda: NSApplication.sharedApplication().activateIgnoringOtherApps_(True))
    except Exception:
        pass
