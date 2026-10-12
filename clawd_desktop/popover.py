"""The small panel under the menu-bar icon (macOS): Clawd and two usage bars.

Left click on the icon opens it, right click (or control-click) still shows the
menu. The panel is a WKWebView (ui/popover.html) in an NSPopover inside the tray
process, so it opens at once and needs no second process; pywebview's WebKit
bindings are already in the app. Windows and Linux keep the menu (left click
there runs its default item, "Cài đặt…").

The page gets everything from `view()` below (pure, tested) and Clawd's sprites
from the clock's own /clawd.pak, read once into the config folder (`load_pak`).
"""
from __future__ import annotations

import contextlib
import importlib
import json
import sys
import threading
import time
from base64 import b64encode
from pathlib import Path
from typing import Any, Callable

from clawd_desktop import paths
from clawd_desktop import preview as pv

UI_DIR = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent.parent)) / "clawd_desktop" / "ui"
WIDTH = 264
STALE = 600.0          # usage older than this says when it was read
PAK_RETRY_S = 300.0    # after a failed read of /clawd.pak


_OBJC = frozenset({"AppKit", "Foundation", "WebKit", "objc", "PyObjCTools.AppHelper"})


def _objc(name: str) -> Any:
    """A PyObjC module as Any: it has no type stubs, and its classes appear at run time."""
    if name not in _OBJC:
        raise ImportError(name)
    return importlib.import_module(name)


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] [popover] {msg}", flush=True)


# =============================================================================
# What the page shows (pure)
# =============================================================================
def left(minutes: float) -> str:
    """Time until a reset, short: 45p, 1g05, 4n 20g."""
    m = max(0, int(minutes))
    if m < 60:
        return f"{m}p"
    if m < 1440:
        return f"{m // 60}g{m % 60:02d}"
    return f"{m // 1440}n {m % 1440 // 60}g"


def view(snap: dict, now: float | None = None) -> dict:
    """{state, label, rows: [{name, pct, left}]} for ui/popover.html.

    `label` is the tray's headline when something needs the user, otherwise
    "Đã kết nối"; usage comes from Claude, so the bars show even without a clock.
    Reset times count down from when the payload was read."""
    from clawd_desktop.tray import status     # late: tray imports this module
    now = time.time() if now is None else now
    state, head, _detail = status(snap, now)
    label = "Đã kết nối" if state == "ok" else head
    p = snap.get("payload") or {}
    at = snap.get("payload_at")
    rows: list[dict] = []
    if isinstance(p.get("s"), (int, float)) and p.get("ok", True) and at:
        gone = max(0.0, (now - at) / 60)

        def row(name: str, pct: Any, rem: Any, when: str = "") -> dict:
            pct = max(0, min(100, int(round(pct)))) if isinstance(pct, (int, float)) else 0
            ok = isinstance(rem, (int, float)) and rem >= 0
            return {"name": name, "pct": pct, "left": when or (left(rem - gone) if ok else "")}

        if p.get("acct") == "ent":          # one spending limit per billing month
            rows.append(row("Tháng", p["s"], p.get("sr"), str(p.get("rd") or "")))
        else:
            rows.append(row("Phiên", p["s"], p.get("sr")))
            rows.append(row("Tuần", p.get("w", 0), p.get("wr")))
        if now - at >= STALE:
            label += time.strftime(" · số liệu lúc %H:%M", time.localtime(at))
    return {"state": state, "label": label, "rows": rows}


# =============================================================================
# Clawd's sprites: the clock's /clawd.pak, cached like the settings window does
# =============================================================================
_pak_failed = 0.0


def load_pak(base: str, key: str, api_factory: Callable[..., Any] | None = None,
             keychain: Any = None, cache_path: Path | None = None,
             now: Callable[[], float] = time.time) -> bytes | None:
    """The cached pack, else read it off the clock (login from the keychain when set).
    None when there is none to be had; the page then draws the app icon."""
    global _pak_failed
    cache = pv.PakCache(cache_path or paths.PAK_FILE)
    have = cache.load()
    if have:
        return have[0]
    if not base or now() - _pak_failed < PAK_RETRY_S:
        return None
    if api_factory is None:
        from clawd_desktop.clockapi import ClockApi
        api_factory = ClockApi
    if keychain is None:
        from clawd_desktop import creds as keychain
    api = api_factory(base, auth=keychain.get(key) if key else None)
    try:
        want = api.heap().get("pak")
        if not isinstance(want, int) or isinstance(want, bool) or want <= 0:
            raise ValueError("the clock has no clawd.pak")
        data = api.get_file("/clawd.pak", pv.PAK_MAX)
        pv.pak_count(data)
    except Exception as e:                  # ClockError, ValueError, network: try again later
        _pak_failed = now()
        log(f"no clawd.pak from {base}: {e}")
        return None
    finally:
        with contextlib.suppress(Exception):
            api.close()
    with contextlib.suppress(OSError):
        cache.save(data, want)
    return data


def supported() -> bool:
    if sys.platform != "darwin":
        return False
    try:
        import WebKit  # noqa: F401  # type: ignore[import-not-found]
    except ImportError:
        return False
    return True


# =============================================================================
# macOS: NSPopover + WKWebView hung on pystray's status item
# =============================================================================
class MacPopover:
    """All methods run on the main thread (tray.on_main)."""

    def __init__(self, status_item: Any, snapshot: Callable[[], dict],
                 pak: Callable[[], bytes | None], on_settings: Callable[[], None],
                 on_menu: Callable[[], None]) -> None:
        AppKit, WebKit, Foundation = _objc("AppKit"), _objc("WebKit"), _objc("Foundation")
        NSURL, NSMakeRect, NSMakeSize = Foundation.NSURL, Foundation.NSMakeRect, Foundation.NSMakeSize
        self._AppKit = AppKit
        self._NSMakeSize = NSMakeSize
        self._item = status_item
        self._snapshot, self._pak, self._on_settings, self._on_menu = snapshot, pak, on_settings, on_menu
        self._ready = False
        self._pak_sent = False
        self._pak_loading = False

        bridge = _classes()["Bridge"].alloc().init()
        bridge.handler = self._message
        cfg = WebKit.WKWebViewConfiguration.alloc().init()
        cfg.userContentController().addScriptMessageHandler_name_(bridge, "clawd")
        self._web = WebKit.WKWebView.alloc().initWithFrame_configuration_(NSMakeRect(0, 0, WIDTH, 92), cfg)
        self._web.setValue_forKey_(False, "drawsBackground")
        vc = AppKit.NSViewController.alloc().init()
        vc.setView_(self._web)
        pop = AppKit.NSPopover.alloc().init()
        pop.setBehavior_(AppKit.NSPopoverBehaviorTransient)
        pop.setAnimates_(True)
        pop.setAppearance_(AppKit.NSAppearance.appearanceNamed_(AppKit.NSAppearanceNameDarkAqua))
        pop.setContentViewController_(vc)
        pop.setContentSize_(NSMakeSize(WIDTH, 92))
        delegate = _classes()["Delegate"].alloc().init()
        delegate.handler = self._closed
        pop.setDelegate_(delegate)
        self._pop, self._keep = pop, (bridge, delegate)
        self._web.loadFileURL_allowingReadAccessToURL_(
            NSURL.fileURLWithPath_(str(UI_DIR / "popover.html")), NSURL.fileURLWithPath_(str(UI_DIR)))

        # Clicks: left -> panel, right / control-click -> menu (pystray's menu is
        # detached from the item, or AppKit would open it on every click).
        clicker = _classes()["Clicker"].alloc().init()
        clicker.handler = self._click
        self._keep += (clicker,)
        button = status_item.button()
        button.setTarget_(clicker)
        button.setAction_(b"clicked:")
        button.sendActionOn_(AppKit.NSEventMaskLeftMouseUp | AppKit.NSEventMaskRightMouseUp)
        status_item.setMenu_(None)

    # ---- clicks -----------------------------------------------------------------
    def _click(self) -> None:
        AK = self._AppKit
        ev = AK.NSApplication.sharedApplication().currentEvent()
        right = ev is not None and (ev.type() == AK.NSEventTypeRightMouseUp
                                    or bool(ev.modifierFlags() & AK.NSEventModifierFlagControl))
        if right:
            if self._pop.isShown():
                self._pop.close()               # at once: the menu below holds the main thread
            self._on_menu()
        elif self._pop.isShown():
            self.close()
        else:
            self.show()

    def show_menu(self, nsmenu: Any) -> None:
        """Open `nsmenu` under the icon the way AppKit does for a status item."""
        if nsmenu is None:
            return
        self._item.setMenu_(nsmenu)
        self._item.button().performClick_(None)     # returns when the menu closes
        self._item.setMenu_(None)

    # ---- the panel --------------------------------------------------------------
    def show(self) -> None:
        AK = self._AppKit
        AK.NSApplication.sharedApplication().activateIgnoringOtherApps_(True)
        button = self._item.button()
        self._pop.showRelativeToRect_ofView_preferredEdge_(button.bounds(), button, AK.NSMinYEdge)
        win = self._web.window()
        if win is not None:
            win.makeKeyWindow()
        self.push()
        self._js("window.clawdVisible && clawdVisible(true)")
        self._want_pak()

    def close(self) -> None:
        if self._pop.isShown():
            self._pop.performClose_(None)

    def _closed(self) -> None:
        self._js("window.clawdVisible && clawdVisible(false)")

    def push(self) -> None:
        """Send the current numbers (cheap; the tray calls it every few seconds)."""
        if self._ready and self._pop.isShown():
            self._js(f"clawdUpdate({json.dumps(view(self._snapshot()), ensure_ascii=False)})")

    def _want_pak(self) -> None:
        if self._pak_sent or self._pak_loading:
            return
        self._pak_loading = True

        def work() -> None:
            data = None
            try:
                data = self._pak()
            finally:
                _objc("PyObjCTools.AppHelper").callAfter(self._got_pak, data)
        threading.Thread(target=work, name="clawd-pak", daemon=True).start()

    def _got_pak(self, data: bytes | None) -> None:
        self._pak_loading = False
        if data and self._ready:
            self._js(f"clawdPak({json.dumps(b64encode(data).decode())})")
            self._pak_sent = True

    # ---- page -> app ----------------------------------------------------------------
    def _message(self, body: Any) -> None:
        try:
            msg = json.loads(str(body)) if not isinstance(body, dict) else dict(body)
        except ValueError:
            return
        cmd = msg.get("cmd")
        if cmd == "ready":
            self._ready = True
            self.push()
            if self._pop.isShown():
                self._js("clawdVisible(true)")
                self._want_pak()
        elif cmd == "size":
            h = msg.get("h")
            if isinstance(h, (int, float)) and 40 <= h <= 400:
                self._pop.setContentSize_(self._NSMakeSize(WIDTH, int(h)))
        elif cmd == "settings":
            self.close()
            self._on_settings()
        elif cmd == "close":
            self.close()

    def _js(self, code: str) -> None:
        self._web.evaluateJavaScript_completionHandler_(code, None)


_CLASSES: dict[str, Any] = {}


def _classes() -> dict[str, Any]:
    """The Objective-C helper classes, defined once per process (AppKit refuses a
    second class of the same name)."""
    if _CLASSES:
        return _CLASSES
    objc = _objc("objc")
    # PyObjC loads frameworks lazily: touch a class so the protocols below exist
    loaded = (_objc("AppKit").NSPopover, _objc("WebKit").WKWebView)
    if not all(loaded):
        raise ImportError("AppKit/WebKit")
    NSObject: Any = _objc("Foundation").NSObject

    class ClawdPopoverBridge(NSObject):
        __pyobjc_protocols__ = [objc.protocolNamed("WKScriptMessageHandler")]

        def userContentController_didReceiveScriptMessage_(self, _ucc, message):
            self.handler(message.body())

    class ClawdPopoverDelegate(NSObject):
        __pyobjc_protocols__ = [objc.protocolNamed("NSPopoverDelegate")]

        def popoverDidClose_(self, _note):
            self.handler()

    class ClawdStatusClicker(NSObject):
        def clicked_(self, _sender):
            self.handler()

    _CLASSES.update(Bridge=ClawdPopoverBridge, Delegate=ClawdPopoverDelegate, Clicker=ClawdStatusClicker)
    return _CLASSES
