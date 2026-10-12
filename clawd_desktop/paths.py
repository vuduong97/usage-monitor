"""Where the app keeps its files. Shared with the Clawdmeter daemon's config dir,
so an existing `~/.config/claude-usage-monitor/config` keeps working."""
from __future__ import annotations

import json
import sys
from pathlib import Path

CONFIG_DIR = Path.home() / ".config" / "claude-usage-monitor"
CONFIG_FILE = CONFIG_DIR / "config"          # optional key = value overrides (see README)
CLOCK_FILE = CONFIG_DIR / "clock.json"       # the remembered clock
PREFS_FILE = CONFIG_DIR / "desktop.json"     # app preferences (first run, toggles)
LOG_FILE = CONFIG_DIR / "clawd-desktop.log"
HOOK_SCRIPT = CONFIG_DIR / "clawd-event.sh"  # copied here so app updates never break hooks
LOG_MAX = 1_000_000


def frozen() -> bool:
    return bool(getattr(sys, "frozen", False))


def load_prefs() -> dict:
    try:
        data = json.loads(PREFS_FILE.read_text())
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def save_prefs(data: dict) -> None:
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    tmp = PREFS_FILE.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, indent=2))
    tmp.replace(PREFS_FILE)


SCREEN_USAGE = 1                              # the clock's `usage.screen`: 0 auto, 1 usage, 2 clawd


def clock_screen() -> int | None:
    """The clock's "Màn hình chính" as the settings window last saw it, or None.
    The settings window is its own process, so this file is how the engine learns
    it (`Brain` stops pushing animations while it is Usage)."""
    v = load_prefs().get("clock_screen")
    return v if isinstance(v, int) and not isinstance(v, bool) else None


def set_clock_screen(screen: int) -> None:
    prefs = load_prefs()
    if prefs.get("clock_screen") != screen:
        prefs["clock_screen"] = screen
        save_prefs(prefs)


class RotatingLog:
    """A text stream for print(): appends to LOG_FILE and keeps one previous
    file, so the log never grows past ~2 MB however long the app runs."""

    def __init__(self, path: Path = LOG_FILE, limit: int = LOG_MAX) -> None:
        self.path, self.limit = path, limit
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._f = open(self.path, "a", encoding="utf-8", buffering=1)

    def write(self, s: str) -> int:
        try:
            n = self._f.write(s)
            if self._f.tell() > self.limit:
                self._f.close()
                self.path.replace(self.path.with_suffix(".log.1"))
                self._f = open(self.path, "a", encoding="utf-8", buffering=1)
            return n
        except (OSError, ValueError):
            return 0

    def flush(self) -> None:
        try:
            self._f.flush()
        except (OSError, ValueError):
            pass

    def isatty(self) -> bool:
        return False


def open_log() -> None:
    """Send print() output to the log file (the packaged app has no console)."""
    log = RotatingLog()
    sys.stdout = log  # type: ignore[assignment]
    sys.stderr = log  # type: ignore[assignment]
