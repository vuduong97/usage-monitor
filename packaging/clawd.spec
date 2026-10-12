# PyInstaller spec: `pyinstaller packaging/clawd.spec` from the repo root.
# macOS -> dist/Clawd.app (menu-bar only, no Dock icon); Windows -> dist/Clawd/Clawd.exe
import sys
from pathlib import Path

from PyInstaller.utils.hooks import collect_submodules

ROOT = Path(SPECPATH).parent
sys.path.insert(0, str(ROOT))
from clawd_desktop import __version__  # noqa: E402

ICONS = ROOT / "build" / "icons"
hidden = collect_submodules("zeroconf") + collect_submodules("clawd_desktop")
if sys.platform == "darwin":
    hidden += ["pystray._darwin", "PyObjCTools.AppHelper"]
    icon = str(ICONS / "clawd.icns")
elif sys.platform == "win32":
    hidden += ["pystray._win32"]
    icon = str(ICONS / "clawd.ico")
else:
    hidden += ["pystray._xorg", "pystray._appindicator"]
    icon = None

a = Analysis(
    [str(ROOT / "packaging" / "entry.py")],
    pathex=[str(ROOT)],
    hiddenimports=hidden,
    excludes=["tkinter", "bleak", "pytest", "numpy"],
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
