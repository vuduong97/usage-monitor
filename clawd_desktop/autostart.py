"""Start the app at login: a LaunchAgent on macOS, an HKCU Run value on Windows.
Both are per-user and need no admin rights."""
from __future__ import annotations

import plistlib
import sys
from pathlib import Path

from clawd_desktop import paths

LABEL = "vn.clawd.desktop"
PLIST = Path.home() / "Library" / "LaunchAgents" / f"{LABEL}.plist"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
RUN_VALUE = "Clawd"

try:
    import winreg  # type: ignore[import]
except ImportError:  # not Windows
    winreg = None  # type: ignore[assignment]


def command() -> list[str]:
    """How to start the app: the packaged executable, or this Python + module."""
    if paths.frozen():
        return [sys.executable]
    return [sys.executable, "-m", "clawd_desktop"]


def supported(platform: str = sys.platform) -> bool:
    return platform in ("darwin", "win32")


def launch_agent(cmd: list[str]) -> dict:
    agent = {
        "Label": LABEL,
        "ProgramArguments": cmd,
        "RunAtLoad": True,
        # Restart after a crash, not after "Thoát" (a clean exit).
        "KeepAlive": {"SuccessfulExit": False},
        "ThrottleInterval": 30,
        "ProcessType": "Interactive",
    }
    if not paths.frozen():
        agent["WorkingDirectory"] = str(Path(__file__).resolve().parents[1])
    return agent


def is_enabled(platform: str = sys.platform) -> bool:
    if platform == "darwin":
        return PLIST.exists()
    if platform == "win32" and winreg is not None:
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as k:
                winreg.QueryValueEx(k, RUN_VALUE)
            return True
        except OSError:
            return False
    return False


def enable(platform: str = sys.platform) -> bool:
    cmd = command()
    if platform == "darwin":
        PLIST.parent.mkdir(parents=True, exist_ok=True)
        with open(PLIST, "wb") as f:
            plistlib.dump(launch_agent(cmd), f)
        return True
    if platform == "win32" and winreg is not None:
        value = " ".join(f'"{c}"' for c in cmd)
        with winreg.CreateKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as k:
            winreg.SetValueEx(k, RUN_VALUE, 0, winreg.REG_SZ, value)
        return True
    return False


def disable(platform: str = sys.platform) -> bool:
    if platform == "darwin":
        PLIST.unlink(missing_ok=True)
        return True
    if platform == "win32" and winreg is not None:
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as k:
                winreg.DeleteValue(k, RUN_VALUE)
        except OSError:
            pass
        return True
    return False


def set_enabled(on: bool, platform: str = sys.platform) -> bool:
    return enable(platform) if on else disable(platform)
