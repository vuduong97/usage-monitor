"""The settings window: `Clawd settings [--clock http://10.0.0.5] [--key clawd]`.

Its own process (pywebview needs the main thread, which pystray already owns
in the tray process), started by the tray's "Cài đặt…" item, so it costs
nothing while closed. The tray talks to it over the child's stdin/stdout:

  tray -> window   "show"                         bring the window forward
                   "show update"                  ... on the Update tab
  window -> tray   {"cmd": "refresh"}             autostart/hooks changed
                   {"cmd": "app_update_apply"}      restart into the downloaded app version
                   {"cmd": "set_clock", "host": "10.0.0.5" | ""}   typed clock / search again

Started by hand (no tray) it works the same; set_clock then edits clock.json
directly, which the next tray start picks up.
"""
from __future__ import annotations

import json
import os
import sys
import threading
import time
from pathlib import Path

from clawd_desktop import APP_NAME, appupdate, feed, firmware, paths, preview
from clawd_desktop.settings_api import SettingsApi

UI_DIR = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent.parent)) / "clawd_desktop" / "ui"
WIDTH, HEIGHT = 820, 600


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] [settings] {msg}", flush=True)


def command(clock: str, key: str, tab: str = "") -> list[str]:
    """How the tray starts this window (frozen app: the same binary)."""
    head = [sys.executable] if paths.frozen() else [sys.executable, "-m", "clawd_desktop"]
    return head + ["settings", "--piped", "--clock", clock, "--key", key] + (["--tab", tab] if tab else [])


class Channel:
    """The pipe to the tray, or (started by hand) direct edits of clock.json."""

    def __init__(self, out, piped: bool) -> None:
        self.out, self.piped = out, piped
        self._lock = threading.Lock()

    def send(self, msg: dict) -> bool:
        """True when the tray got the message."""
        if self.piped:
            with self._lock:
                try:
                    self.out.write(json.dumps(msg) + "\n")
                    self.out.flush()
                    return True
                except (OSError, ValueError, AttributeError):
                    pass
        if msg.get("cmd") == "set_clock":
            from clawd_desktop.discovery import ClockStore
            store = ClockStore(paths.CLOCK_FILE)
            store.save({"manual_host": msg["host"]} if msg.get("host") else {})
        return False


def dev() -> bool:
    """`make dev`: CLAWD_DEV=1 from source turns on the inspector and live reload."""
    return os.environ.get("CLAWD_DEV") == "1" and not paths.frozen()


def _ui_stamp() -> dict[Path, int]:
    return {p: p.stat().st_mtime_ns for p in UI_DIR.rglob("*") if p.is_file()}


def _watch_ui(win) -> None:
    """Dev: reload the page when a file under ui/ changes. Python changes restart
    the whole process instead (tools/dev.py)."""
    last = _ui_stamp()
    while True:
        time.sleep(0.3)
        try:
            now = _ui_stamp()
        except OSError:                     # an editor's atomic save: the file is gone for a moment
            continue
        if now != last:
            last = now
            log("ui changed: reload")
            try:
                win.evaluate_js("location.reload()")
            except Exception as e:          # the window may be closing
                log(f"reload failed: {e!r}")


def main(clock: str, key: str, piped: bool, tab: str = "") -> int:
    pipe_out = sys.stdout                  # the tray reads commands here; keep it before logging moves stdout
    if paths.frozen() or not sys.stdout or not sys.stdout.isatty():
        paths.open_log()                    # started by hand in a terminal: log there (as the tray does)
    log(f"window for {clock or 'no clock'}")
    import webview

    chan = Channel(pipe_out, piped)
    bundle = firmware.load_bundle()
    renderer = preview.load_renderer(firmware_commit=bundle.commit if bundle else "")
    log("preview: " + (f"firmware renderer {renderer.commit or '?'}" if renderer else "no renderer, approximate canvas"))
    source = feed.from_settings()
    log("update feed: " + (f"{source.url} ({source.channel})" if source else "none"))
    up = appupdate.updater()
    log("app update: " + (f"{up.source.repo}" if up.source else "off"))
    api = SettingsApi(clock, key, notify=chan.send, bundle=bundle, renderer=renderer, feed=source, tab=tab,
                      updater=up)
    win = webview.create_window(
        f"{APP_NAME} · Cài đặt", url=str(UI_DIR / "index.html"), js_api=api,
        width=WIDTH, height=HEIGHT, min_size=(720, 540), background_color="#141413",
        text_select=False)
    if win is None:
        log("could not create the window")
        return 1
    api._window = win

    def closing() -> bool:
        if api.busy():                      # never leave a firmware upload half done
            win.evaluate_js("window.clawdBusyClose && window.clawdBusyClose()")
            return False
        return True
    win.events.closing += closing

    def listen() -> None:                   # "show" from the tray
        for line in sys.stdin:
            words = line.split()
            if words[:1] == ["show"]:
                try:
                    if words[1:] == ["update"]:
                        win.evaluate_js("window.clawdShowTab && window.clawdShowTab('update')")
                    win.restore()
                    win.show()
                    _front()
                except Exception as e:      # the window may be closing
                    log(f"show failed: {e!r}")
    if piped:
        threading.Thread(target=listen, name="clawd-settings-in", daemon=True).start()

    if dev():
        webview.settings["OPEN_DEVTOOLS_IN_DEBUG"] = False   # right click -> Inspect Element when needed
        threading.Thread(target=_watch_ui, args=(win,), name="clawd-settings-dev", daemon=True).start()
        log("dev: live reload on " + str(UI_DIR))
    webview.start(_front, private_mode=True, debug=dev())
    log("window closed")
    return 0


def _front() -> None:
    """macOS: a menu-bar-only app's window opens behind the frontmost app."""
    if sys.platform != "darwin":
        return
    try:
        from AppKit import NSApplication  # type: ignore[attr-defined]  # pyobjc, no stubs
        from PyObjCTools import AppHelper
        AppHelper.callAfter(lambda: NSApplication.sharedApplication().activateIgnoringOtherApps_(True))
    except Exception as e:                  # only cosmetic: the window opens behind
        log(f"bring to front failed: {e!r}")
