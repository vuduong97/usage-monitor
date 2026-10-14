"""Where the app keeps its files. Shared with the older usage daemon's config dir,
so an existing `~/.config/claude-usage-monitor/config` keeps working."""
from __future__ import annotations

import json
import sys
import time
from pathlib import Path

CONFIG_DIR = Path.home() / ".config" / "claude-usage-monitor"
CONFIG_FILE = CONFIG_DIR / "config"          # optional key = value overrides (see README)
CLOCK_FILE = CONFIG_DIR / "clock.json"       # the remembered clock
PREFS_FILE = CONFIG_DIR / "desktop.json"     # app preferences (first run, toggles)
PAYLOAD_FILE = CONFIG_DIR / "usage_payload.json"   # the last usage payload sent to the clock
LOG_FILE = CONFIG_DIR / "clawd-desktop.log"
PAK_FILE = CONFIG_DIR / "clawd.pak"           # the clock's Clawd sprites, read from the clock (preview)
HOOK_SCRIPT = CONFIG_DIR / "clawd-event.sh"  # copied here so app updates never break hooks
OTA_BUSY_FILE = CONFIG_DIR / "ota.busy"      # exists while the settings window installs firmware on the clock
OTA_BUSY_STALE = 900.0                        # ... unless nobody touched it for this long (s)
LOG_MAX = 1_000_000


def frozen() -> bool:
    return bool(getattr(sys, "frozen", False))


def integration(rel: str) -> Path:
    """A file the app copies into an agent (integrations/pi/clawd-home.ts, the opencode
    plugin): bundled into the frozen app (packaging/clawd.spec), else the repo's copy."""
    if frozen():
        return Path(getattr(sys, "_MEIPASS", ".")) / "clawd_desktop" / "integrations" / rel
    return Path(__file__).resolve().parent.parent / "integrations" / rel


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


def set_ota_busy(on: bool, path: Path | None = None) -> None:
    """Mark (or touch, or clear) a firmware install in progress. The tray reads it so an app
    update never restarts the app in the middle of an upload to the clock."""
    path = path or OTA_BUSY_FILE
    try:
        if on:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(str(time.time()))
        else:
            path.unlink(missing_ok=True)
    except OSError:
        pass


def ota_busy(path: Path | None = None, now: float | None = None) -> bool:
    try:
        age = (time.time() if now is None else now) - (path or OTA_BUSY_FILE).stat().st_mtime
    except OSError:
        return False
    return age < OTA_BUSY_STALE


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


# The keys the clock's /api/usage reads for its usage screen. Alerts (m, x) and the
# beep (b) are one-shot and never stored: the settings preview must not replay them.
PAYLOAD_KEYS = ("s", "sr", "w", "wr", "st", "ok", "acct", "tp", "rd", "t", "tf")


def set_usage_payload(payload: dict, now: float | None = None, path: Path | None = None) -> None:
    """Keep the last usage payload the engine delivered, for the settings window
    (its own process) to draw the clock's screen from. Atomic, like the prefs."""
    path = path or PAYLOAD_FILE
    data = {"at": time.time() if now is None else now,
            "payload": {k: payload[k] for k in PAYLOAD_KEYS if k in payload}}
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(data))
    tmp.replace(path)


def usage_payload(path: Path | None = None) -> tuple[dict, float] | None:
    """(payload, time it was sent) or None when the engine has not sent one yet."""
    try:
        data = json.loads((path or PAYLOAD_FILE).read_text())
        payload, at = data["payload"], data["at"]
    except (OSError, ValueError, KeyError, TypeError):
        return None
    if not isinstance(payload, dict) or isinstance(at, bool) or not isinstance(at, (int, float)):
        return None
    return payload, float(at)


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
