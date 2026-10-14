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
from typing import Any

from clawd_desktop import APP_NAME, __version__, appupdate, autostart, hooks, icons, paths
from clawd_desktop.engine import Engine, daemon_running

REFRESH = 3.0
FEED_FIRST = 60.0       # first firmware feed check after start (s)
FEED_RETRY = 1800.0     # after a failed check
AGENT_SCAN = 600.0      # look for newly installed coding agents this often (s)
FRESH = 180.0      # a push this recent counts as "connected"
ADDRESS_PROMPT = ("Địa chỉ IP của đồng hồ (hiện trên màn hình đồng hồ lúc khởi động), "
                  "ví dụ 192.168.1.48:")


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


# =============================================================================
# Status text (pure, tested)
# =============================================================================
def _no(v: object) -> bool:
    """An explicit False (None means "not known yet")."""
    return isinstance(v, bool) and not v


def status(snap: dict, now: float | None = None) -> tuple[str, str, str]:
    """(dot state, headline, detail line) for the menu and tooltip."""
    now = time.time() if now is None else now
    clock = snap.get("clock")
    last_ok = snap.get("last_ok")
    connected = bool(last_ok and now - last_ok < FRESH)
    payload = snap.get("payload") or {}
    detail = ""
    at = snap.get("payload_at")
    if not _no(payload.get("ok")) and ("s" in payload or "w" in payload) and at:
        detail = f"Phiên {payload.get('s', 0)}% · Tuần {payload.get('w', 0)}%"
        if now - at >= FRESH:          # only an old reading needs its time
            detail += time.strftime(" · lúc %H:%M", time.localtime(at))

    if _no(snap.get("token")):
        return "error", "Chưa đăng nhập Claude: mở Terminal, gõ claude login", detail
    if snap.get("manual_config"):
        where = ", ".join(snap.get("config_targets") or [])
        return ("ok" if connected else "busy",
                ("Đã kết nối · " if connected else "Đang gửi tới · ") + where, detail)
    if clock is None:      # nothing to try: no address saved and mDNS silent (there is no scan)
        return "error", "Không thấy đồng hồ: nhập IP hiện trên màn hình đồng hồ (Nhập địa chỉ đồng hồ…)", detail
    name = clock.name or clock.host    # the address lives in Cài đặt
    if connected:
        return "ok", f"Đã kết nối · {name}", detail
    if snap.get("failing"):
        return "error", f"Đồng hồ {name} không trả lời: đổi IP thì nhập IP mới (Nhập địa chỉ đồng hồ…)", detail
    return "busy", f"Đang kết nối {name}…", detail


def lost(snap: dict, now: float | None = None) -> bool:
    """The clock is not reached (none known, or no push got through lately): the menu offers
    to type its address. Not for `http_targets` from the config file."""
    now = time.time() if now is None else now
    last_ok = snap.get("last_ok")
    return not snap.get("manual_config") and not (last_ok and now - last_ok < FRESH)


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
            os.startfile(path)  # type: ignore[attr-defined]  # noqa: S606 (opens a file, no shell wanted)
        else:
            subprocess.Popen(["xdg-open", path])
    except OSError:
        pass


# =============================================================================
# First run
# =============================================================================
def first_run() -> None:
    """Autostart on, once. Nothing of any agent is touched here: the setup wizard asks
    first (firstrun.py). Plugin copies older than this app are refreshed and the hook
    script rewritten, so an app update reaches agents already connected."""
    from clawd_desktop import agents, firstrun
    prefs = firstrun.load()
    if not prefs.get("first_run_done"):
        if autostart.supported():
            try:
                autostart.enable()
                log("Autostart enabled")
            except OSError as e:
                log(f"Autostart not enabled: {e}")
        prefs = firstrun.load()
        prefs["first_run_done"] = True
        paths.save_prefs(prefs)
    try:
        if sys.platform != "win32" and paths.HOOK_SCRIPT.exists():
            hooks.write_script()
        for aid in agents.refresh_plugins():
            log(f"Updated the {aid} plugin")
    except Exception as e:                  # never keep the app from starting
        log(f"Agent refresh: {e!r}")


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
        M: Any = pystray.MenuItem   # `visible` takes a callable too; the stubs say bool
        self.icon = pystray.Icon(
            "clawd", self._icons["busy"], APP_NAME,
            menu=pystray.Menu(
                M(lambda _i: self.head, None, enabled=False),
                M(lambda _i: self.detail, None, enabled=False, visible=lambda _i: bool(self.detail)),
                pystray.Menu.SEPARATOR,
                M(lambda _i: f"● Khởi động lại để cập nhật Clawd {self.app_new}", self.restart_update,
                  visible=lambda _i: bool(self.app_new)),
                M(lambda _i: f"● Có firmware mới (#{self.fw_new})", self.open_update,
                  visible=lambda _i: bool(self.fw_new)),
                M("● Hoàn tất thiết lập…", self.open_setup, visible=lambda _i: bool(self.setup_left)),
                M(lambda _i: f"● Tìm thấy {self._new_names()}: nối với Clawd…", self.open_agents,
                  visible=lambda _i: bool(self.new_agents)),
                M("Cài đặt…", self.open_settings, default=True),
                # Only while the clock is not answering. Everything else (retry, address,
                # log, the clock's web page, version) lives in the settings window; this one
                # works without it too (e.g. no WebView2).
                M("Nhập địa chỉ đồng hồ…", self.manual, visible=lambda _i: self._lost()),
                pystray.Menu.SEPARATOR,
                M(f"Thoát {APP_NAME}", self.quit),
            ))
        self._stop = threading.Event()
        self._settings: subprocess.Popen | None = None
        self._popover: Any = None       # macOS: the panel a left click opens (popover.py)
        self.fw_new = 0                 # a newer release for the clock (feed.py), 0: none
        self.app_new = ""               # a newer app, downloaded and checked (appupdate.py), "": none
        self.updater = appupdate.updater()
        self.setup_left: list[str] = []  # wizard steps not done yet (firstrun.py)
        self.new_agents: list[str] = []  # agents installed since last offered, not connected

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
        self._sync_app_update()
        if self._popover is not None:
            self.on_main(self._popover.push)     # a no-op while the panel is closed

    def _sync_app_update(self) -> None:
        """The menu item follows the folder, so a download made by the settings window shows too."""
        try:
            st = self.updater.state()
        except Exception as e:                   # never let the updater break the tray
            log(f"App update state: {e!r}")
            return
        new = st["version"] if st["status"] == "ready" and st["canApply"] else ""
        if new != self.app_new:
            self.app_new = new
            log(f"App update ready: {new}" if new else "No app update waiting")
            self.on_main(self.icon.update_menu)

    # ---- the panel (macOS) -------------------------------------------------------
    def _install_popover(self) -> None:
        """Left click opens the panel, right click the menu. Runs on the main thread; on
        any failure the icon keeps its plain menu."""
        from clawd_desktop import popover
        if not popover.supported():
            return
        icon: Any = self.icon
        try:
            self._popover = popover.MacPopover(
                icon._status_item, self.engine.snapshot, self._clawd_pak,
                on_settings=self.open_settings, on_menu=self._show_menu)
        except Exception as e:
            log(f"Panel not available, the menu stays: {e!r}")
            self._popover = None
            return
        update = icon._update_menu

        def detached() -> None:              # pystray re-attaches the menu on every update
            update()
            icon._status_item.setMenu_(None)
        icon._update_menu = detached
        log("Panel ready (left click), menu on right click")

    def _show_menu(self) -> None:
        icon: Any = self.icon
        icon.update_menu()                    # current texts
        handle = getattr(icon, "_menu_handle", None)
        if self._popover is not None and handle:
            self._popover.show_menu(handle[0])

    def _clawd_pak(self) -> bytes | None:
        from clawd_desktop import popover
        clock = self.engine.snapshot().get("clock")
        return popover.load_pak(clock.base, self._clock_key(clock)) if clock is not None else popover.load_pak("", "")

    def _loop(self) -> None:
        while not self._stop.wait(REFRESH):
            try:
                self.refresh()
            except Exception as e:  # never let a status glitch kill the tray
                log(f"Tray refresh failed: {e!r}")

    # ---- menu actions ------------------------------------------------------------
    def _lost(self) -> bool:
        return lost(self.engine.snapshot())

    def manual(self) -> None:
        def ask() -> None:
            text = ask_text(ADDRESS_PROMPT)
            if text and self.engine.finder.set_manual(text) is None:
                alert(f"“{text}” không phải địa chỉ hợp lệ.")
            self.refresh()
        threading.Thread(target=ask, daemon=True).start()

    def open_update(self) -> None:
        self._open_window("update")

    def _feed_loop(self) -> None:
        """Ask the firmware feed now and then whether the clock has something to install."""
        from clawd_desktop import feed, firmware
        from clawd_desktop.clockapi import ClockApi, ClockError
        source = feed.from_settings()
        if source is None:
            return
        log(f"Update feed: {source.url} ({source.channel})")
        bundle = firmware.load_bundle()
        wait = FEED_FIRST
        while not self._stop.wait(wait):
            wait = feed.CHECK_EVERY
            clock = self.engine.snapshot().get("clock")
            if clock is None:
                wait = FEED_FIRST                  # try again soon once a clock is found
                continue
            try:
                m = source.latest()
                api = ClockApi(clock.base)
                try:
                    heap = api.heap()
                finally:
                    api.close()
            except (feed.FeedError, ClockError) as e:
                log(f"Update check: {e}")
                wait = FEED_RETRY
                continue
            new = feed.offer(heap, m, bundle)
            if new != self.fw_new:
                self.fw_new = new
                log(f"Update for the clock: #{new}" if new else "Clock is up to date")
                self.on_main(self.icon.update_menu)

    def _app_update_loop(self) -> None:
        """Look for a newer app now and then (GitHub Releases) and download it in the background.
        Applying it is the user's choice: the menu item / the Update tab's button."""
        if self.updater.source is None or not appupdate.auto_enabled():
            log("App update check: off")
            return
        log(f"App update check: {self.updater.source.repo}")
        wait = appupdate.FIRST_CHECK
        while not self._stop.wait(wait):
            wait = appupdate.CHECK_EVERY
            try:
                st = self.updater.check(force=True)
            except Exception as e:
                log(f"App update check: {e!r}")
                wait = appupdate.RETRY
                continue
            if st["status"] == "error":
                wait = appupdate.RETRY
            self._sync_app_update()

    def restart_update(self) -> None:
        threading.Thread(target=self._restart_for_update, name="clawd-restart", daemon=True).start()

    def _restart_for_update(self) -> None:
        ok, why = self.updater.apply(os.getpid())
        if not ok:
            alert(why)
            self._sync_app_update()
            return
        log("Restarting to install the update")
        self.quit()

    def open_settings(self) -> None:      # no arguments: pystray passes (icon, item) to more
        self._open_window("")

    def open_setup(self) -> None:
        self._open_window("setup")

    def open_agents(self) -> None:
        from clawd_desktop import firstrun
        firstrun.mark_seen(self.new_agents)   # offered once: the item goes, whatever the answer
        self.new_agents = []
        self.on_main(self.icon.update_menu)
        self._open_window("agents")

    def _maybe_open_setup(self) -> None:
        """Open the wizard by itself once per app version while steps are left."""
        from clawd_desktop import firstrun
        try:
            if not firstrun.should_open():
                return
            firstrun.mark_shown()
        except OSError as e:
            log(f"Setup state: {e}")
            return
        log("Opening the setup wizard")
        self._open_window("setup")

    def _rescan(self) -> None:
        from clawd_desktop import firstrun
        try:
            left = firstrun.pending()
        except Exception as e:
            log(f"Setup state: {e!r}")
            return
        if left != self.setup_left or ("agents" not in left and self.new_agents):
            self.setup_left = left
            self.new_agents = [i for i in self.new_agents if i not in firstrun.seen()]
            self.on_main(self.icon.update_menu)

    def _new_names(self) -> str:
        from clawd_desktop import agents
        reg = agents.registry()
        return ", ".join(reg[i].name for i in self.new_agents if i in reg)

    def _agent_loop(self) -> None:
        """Keep the wizard item and the "new agent found" offer current."""
        from clawd_desktop import agents, firstrun
        wait = 2.0
        while not self._stop.wait(wait):
            wait = AGENT_SCAN
            try:
                left = firstrun.pending()
                new: list[str] = []
                if "agents" not in left:           # the wizard's Agent step offers them itself
                    reg = agents.registry()
                    installed = [i for i, a in reg.items() if _installed(a)]
                    new = firstrun.new_agents(agents.found_ids(reg), installed)
            except Exception as e:                 # never let a scan break the tray
                log(f"Agent scan: {e!r}")
                continue
            if (left, new) != (self.setup_left, self.new_agents):
                if new != self.new_agents and new:
                    log(f"New agents found: {', '.join(new)}")
                self.setup_left, self.new_agents = left, new
                self.on_main(self.icon.update_menu)

    def _open_window(self, tab: str) -> None:
        """The settings window, in its own process (pywebview wants the main
        thread, which the tray holds). A second click brings it forward."""
        from clawd_desktop import settings_app
        p = self._settings
        if p is not None and p.poll() is None and p.stdin is not None:
            try:
                p.stdin.write(f"show {tab}\n".encode() if tab else b"show\n")
                p.stdin.flush()
                return
            except (OSError, ValueError):
                pass
        clock = self.engine.snapshot().get("clock")
        base = clock.base if clock is not None else ""
        key = self._clock_key(clock) if clock is not None else ""
        try:
            p = subprocess.Popen(settings_app.command(base, key, tab), stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, **_no_window())
        except OSError as e:
            log(f"Settings window did not start: {e}")
            return
        self._settings = p
        threading.Thread(target=self._settings_cmds, args=(p,), name="clawd-settings", daemon=True).start()

    def _typed_host(self) -> str:
        return str(self.engine.finder.store.load().get("manual_host") or "")

    def _clock_key(self, clock) -> str:
        """The keychain entry for the clock's web password (its name survives a new IP)."""
        return clock.name or self._typed_host() or clock.host

    def _settings_cmds(self, p: subprocess.Popen) -> None:
        for line in p.stdout or ():
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
                    self.engine.finder.retry()       # "Tìm lại": same address, again now
                self.refresh()
            elif cmd == "use_clock":                # picked in the setup wizard (mDNS)
                from clawd_desktop.discovery import Clock
                try:
                    c = Clock(host=str(msg["host"]), port=int(msg.get("port") or 80),
                              name=str(msg.get("name") or ""), version=str(msg.get("version") or ""),
                              source="mdns")
                except (KeyError, TypeError, ValueError):
                    continue
                self.engine.finder.use(c)
                self.refresh()
            elif cmd == "usage_source":
                self.engine.poll_now()
            elif cmd == "app_update_apply":
                self._restart_for_update()
            elif cmd == "setup":                    # a wizard step or an agent changed
                threading.Thread(target=self._rescan, name="clawd-rescan", daemon=True).start()

    def quit(self) -> None:
        self._stop.set()
        if self._settings is not None and self._settings.poll() is None:
            self._settings.terminate()
        self.engine.stop()
        self.icon.stop()

    def run(self) -> None:
        def setup(icon) -> None:
            icon.visible = True
            if sys.platform == "darwin":
                self.on_main(self._install_popover)
            threading.Thread(target=self._loop, name="clawd-tray", daemon=True).start()
            threading.Thread(target=self._feed_loop, name="clawd-feed", daemon=True).start()
            threading.Thread(target=self._app_update_loop, name="clawd-appupdate", daemon=True).start()
            threading.Thread(target=self._agent_loop, name="clawd-agents", daemon=True).start()
            self.refresh()
            self._maybe_open_setup()
        self.icon.run(setup=setup)


def _installed(a) -> bool:
    try:
        return a.is_installed()
    except Exception:
        return False


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
