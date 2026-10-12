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
FIRMWARE = ROOT / "clawd_desktop" / "firmware"
if os.environ.get("CLAWD_PRIVATE_FIRMWARE") == "1" and (FIRMWARE / "firmware.json").is_file():
    datas.append((str(FIRMWARE), "clawd_desktop/firmware"))
    print(f"bundling clock firmware from {FIRMWARE}")
else:
    print("no clock firmware bundled (CLAWD_PRIVATE_FIRMWARE=1 + tools/bundle_firmware.py); "
          "the Update tab will say so")
if sys.platform == "darwin":
    hidden += ["pystray._darwin", "PyObjCTools.AppHelper", "webview.platforms.cocoa", "keyring.backends.macOS"]
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
exe = EXE(
    pyz, a.scripts, [],
    exclude_binaries=True,
    name="Clawd",
    console=False,
    icon=icon,
    upx=False,
    argv_emulation=False,
)
coll = COLLECT(exe, a.binaries, a.datas, name="Clawd", upx=False)

if sys.platform == "darwin":
    app = BUNDLE(
        coll,
        name="Clawd.app",
        icon=icon,
        bundle_identifier="vn.clawd.desktop",
        version=__version__,
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
            "NSBonjourServices": ["_clawdmeter._tcp"],
        },
    )
