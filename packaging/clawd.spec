# PyInstaller spec: `pyinstaller packaging/clawd.spec` from the repo root.
# macOS -> dist/Clawd.app (menu-bar only, no Dock icon); Windows -> dist/Clawd/Clawd.exe
import os
import sys
from pathlib import Path

from PyInstaller.utils.hooks import collect_submodules

ROOT = Path(SPECPATH).parent
sys.path.insert(0, str(ROOT))
from clawd_desktop import __version__  # noqa: E402

ICONS = ROOT / "build" / "icons"
hidden = collect_submodules("zeroconf") + collect_submodules("clawd_desktop")
# The settings window: its page, and the clock firmware if tools/bundle_firmware.py put one
# there AND the build asks for it. The firmware is private: a build with it is for clock
# owners only, never a public GitHub Release, so it takes an explicit opt-in.
datas = [(str(ROOT / "clawd_desktop" / "ui"), "clawd_desktop/ui")]
# The plugin files the app copies into agents (clawd_desktop/agents, paths.integration).
for rel in ("pi/clawd-home.ts", "opencode/clawd.js"):
    datas.append((str(ROOT / "integrations" / rel), f"clawd_desktop/integrations/{Path(rel).parent}"))
FIRMWARE = ROOT / "clawd_desktop" / "firmware"
if os.environ.get("CLAWD_PRIVATE_FIRMWARE") == "1" and (FIRMWARE / "firmware.json").is_file():
    datas.append((str(FIRMWARE), "clawd_desktop/firmware"))
    print(f"bundling clock firmware from {FIRMWARE}")
    # The clock-screen renderer (tools/build_preview.py) is compiled from the same private
    # firmware, so it rides on the same opt-in.
    PREVIEW = ROOT / "clawd_desktop" / "renderer"
    if (PREVIEW / "preview.json").is_file():
        datas.append((str(PREVIEW), "clawd_desktop/renderer"))
        print(f"bundling the clock-screen renderer from {PREVIEW}")
    else:
        print("no clock-screen renderer built (tools/build_preview.py); the preview will be approximate")
else:
    print("no clock firmware bundled (CLAWD_PRIVATE_FIRMWARE=1 + tools/bundle_firmware.py); "
          "the Update tab will say so")
# The firmware feed (clawd_desktop/feed.py): its URL and app token come from the build
# environment (CI secrets), never from the repo. Without them the app has no feed.
FEED_BUILD = ROOT / "clawd_desktop" / "_feed_build.py"
if os.environ.get("CLAWD_FEED_URL") and os.environ.get("CLAWD_FEED_TOKEN"):
    FEED_BUILD.write_text("# written by packaging/clawd.spec from the build environment; never commit\n"
                          f"URL = {os.environ['CLAWD_FEED_URL']!r}\nTOKEN = {os.environ['CLAWD_FEED_TOKEN']!r}\n")
    hidden.append("clawd_desktop._feed_build")
    print(f"firmware feed: {os.environ['CLAWD_FEED_URL']}")
else:
    FEED_BUILD.unlink(missing_ok=True)
    print("no firmware feed (CLAWD_FEED_URL + CLAWD_FEED_TOKEN); only a bundled firmware can be installed")
if sys.platform == "darwin":
    # clawd_desktop.popover loads AppKit/WebKit by name (importlib): list them for the analysis
    hidden += ["pystray._darwin", "PyObjCTools.AppHelper", "webview.platforms.cocoa", "keyring.backends.macOS",
               "clawd_desktop.popover", "AppKit", "Foundation", "WebKit", "objc"]
    icon = str(ICONS / "clawd.icns")
elif sys.platform == "win32":
    hidden += ["pystray._win32", "webview.platforms.winforms", "webview.platforms.edgechromium",
               "keyring.backends.Windows"]
    icon = str(ICONS / "clawd.ico")
else:
    hidden += ["pystray._xorg", "pystray._appindicator"]
    icon = None

a = Analysis(
    [str(ROOT / "packaging" / "entry.py")],
    pathex=[str(ROOT)],
    hiddenimports=hidden,
    datas=datas,
    # pywebview's Qt/GTK/Android backends are never used: keep them (and their toolkits) out.
    excludes=["tkinter", "bleak", "pytest", "numpy", "webview.platforms.qt", "webview.platforms.gtk",
              "webview.platforms.android", "webview.platforms.cef", "PyQt5", "PyQt6", "PySide2", "PySide6",
              "qtpy", "gi", "cefpython3"],
    noarchive=False,
)
pyz = PYZ(a.pure)
# macOS: CLAWD_SIGN_IDENTITY ("Developer ID Application: ...") signs every binary with the hardened
# runtime, which notarization needs; without it build_mac.sh falls back to an ad-hoc signature.
SIGN = os.environ.get("CLAWD_SIGN_IDENTITY") or None if sys.platform == "darwin" else None
ENTITLEMENTS = str(ROOT / "packaging" / "entitlements.plist") if SIGN else None
exe = EXE(
    pyz, a.scripts, [],
    exclude_binaries=True,
    name="Clawd",
    console=False,
    icon=icon,
    upx=False,
    argv_emulation=False,
    codesign_identity=SIGN,
    entitlements_file=ENTITLEMENTS,
)
coll = COLLECT(exe, a.binaries, a.datas, name="Clawd", upx=False)

if sys.platform == "darwin":
    app = BUNDLE(
        coll,
        name="Clawd.app",
        icon=icon,
        bundle_identifier="vn.clawd.desktop",   # appupdate.BUNDLE_ID: a downloaded update must carry it
        version=__version__,
        codesign_identity=SIGN,
        entitlements_file=ENTITLEMENTS,
        info_plist={
            "CFBundleName": "Clawd",
            "CFBundleDisplayName": "Clawd",
            "CFBundleShortVersionString": __version__,
            "CFBundleVersion": __version__,
            "LSUIElement": True,                  # menu bar only
            "LSMinimumSystemVersion": "11.0",
            "NSHighResolutionCapable": True,
            "NSLocalNetworkUsageDescription":
                "Clawd tìm đồng hồ Clawd trong mạng WiFi nhà bạn để hiển thị mức dùng Claude.",
            "NSBonjourServices": ["_clawd._tcp"],
        },
    )
