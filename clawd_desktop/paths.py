"""Where the app keeps its files. Shared with the older usage daemon's config dir,
so an existing `~/.config/claude-usage-monitor/config` keeps working."""
from __future__ import annotations

import contextlib
import copy
import json
import os
import sys
import threading
import time
from collections.abc import Callable, Iterator
from pathlib import Path

CONFIG_DIR = Path.home() / ".config" / "claude-usage-monitor"
CONFIG_FILE = CONFIG_DIR / "config"          # optional key = value overrides (see README)
CLOCK_FILE = CONFIG_DIR / "clock.json"       # the remembered clock
PREFS_FILE = CONFIG_DIR / "desktop.json"     # app preferences (first run, toggles)
PAYLOAD_FILE = CONFIG_DIR / "usage_payload.json"   # the last usage payload sent to the clock
LOG_FILE = CONFIG_DIR / "clawd-desktop.log"
PAK_FILE = CONFIG_DIR / "clawd.pak"           # the clock's Clawd sprites, read from the clock (preview)
RENDERER_DIR = CONFIG_DIR / "renderer"        # the feed release's clock-screen renderer, unpacked (preview)
HOOK_SCRIPT = CONFIG_DIR / "clawd-event.sh"  # copied here so app updates never break hooks
OTA_BUSY_FILE = CONFIG_DIR / "ota.busy"      # exists while the settings window installs firmware on the clock
OTA_BUSY_STALE = 900.0                        # ... unless nobody touched it for this long (s)
LOG_MAX = 1_000_000
PREFS_IO_WAIT = 5.0                           # how long desktop.json may stay locked or busy (s)


def frozen() -> bool:
    return bool(getattr(sys, "frozen", False))


def integration(rel: str) -> Path:
    """A file the app copies into an agent (integrations/pi/clawd-home.ts, the opencode
    plugin): bundled into the frozen app (packaging/clawd.spec), else the repo's copy."""
    if frozen():
        return Path(getattr(sys, "_MEIPASS", ".")) / "clawd_desktop" / "integrations" / rel
    return Path(__file__).resolve().parent.parent / "integrations" / rel


# desktop.json is written by two processes: the tray's engine and the settings window.
# Every read-modify-write runs under a lock file (update_prefs), so neither drops a key
# the other just saved. On Windows a file being replaced (or still open in the other
# process) answers PermissionError for a moment: that is waited out, never read as empty.

def _retrying(op: Callable[[], object], deadline: float) -> object:
    while True:
        try:
            return op()
        except PermissionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.02)


def _read_prefs() -> dict:
    """desktop.json, {} when missing or corrupt. Raises OSError when it is there but
    stays unreadable: callers that write must not take that for an empty file."""
    try:
        text = _retrying(PREFS_FILE.read_text, time.monotonic() + PREFS_IO_WAIT)
    except FileNotFoundError:
        return {}
    try:
        data = json.loads(str(text))
    except ValueError:
        return {}
    return data if isinstance(data, dict) else {}


def _write_prefs(data: dict) -> None:
    PREFS_FILE.parent.mkdir(parents=True, exist_ok=True)
    # one temp file per writer: a shared name let two processes write the same one
    tmp = PREFS_FILE.with_name(f"{PREFS_FILE.stem}.{os.getpid()}.{threading.get_ident()}.tmp")
    try:
        tmp.write_text(json.dumps(data, indent=2))
        _retrying(lambda: tmp.replace(PREFS_FILE), time.monotonic() + PREFS_IO_WAIT)
    except BaseException:
        with contextlib.suppress(OSError):
            tmp.unlink()
        raise


def _lock_byte(fd: int) -> bool:
    if sys.platform == "win32":
        import msvcrt
        os.lseek(fd, 0, os.SEEK_SET)
        try:
            msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
        except OSError:
            return False
        return True
    import fcntl
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return False
    return True


def _unlock_byte(fd: int) -> None:
    if sys.platform == "win32":
        import msvcrt
        os.lseek(fd, 0, os.SEEK_SET)
        msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
    else:
        import fcntl
        fcntl.flock(fd, fcntl.LOCK_UN)


@contextlib.contextmanager
def _prefs_lock() -> Iterator[None]:
    """Exclusive across processes and threads (each holder opens its own handle);
    not reentrant. TimeoutError (an OSError) after PREFS_IO_WAIT."""
    path = PREFS_FILE.with_suffix(".lock")
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        deadline = time.monotonic() + PREFS_IO_WAIT
        while not _lock_byte(fd):
            if time.monotonic() >= deadline:
                raise TimeoutError(f"{PREFS_FILE.name} is locked by another Clawd process")
            time.sleep(0.02)
        try:
            yield
        finally:
            _unlock_byte(fd)
    finally:
        os.close(fd)


def load_prefs() -> dict:
    """For reading only ({} when it cannot be read); changes go through update_prefs."""
    try:
        return _read_prefs()
    except OSError:
        return {}


def update_prefs(change: Callable[[dict], dict | None]) -> dict:
    """Read desktop.json, apply `change` (edit the dict in place, or return a new one),
    write it back if anything changed, all under the lock; returns the result. Raises
    OSError, writing nothing, when the file cannot be read or the lock not taken."""
    with _prefs_lock():
        prefs = _read_prefs()
        before = copy.deepcopy(prefs)
        out = change(prefs)
        new = prefs if out is None else out
        if new != before or not PREFS_FILE.exists():
            _write_prefs(new)
        return new


def save_prefs(data: dict) -> None:
    """Replace all of desktop.json with `data` (update_prefs to change some keys)."""
    with _prefs_lock():
        _write_prefs(data)


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
    update_prefs(lambda prefs: prefs.__setitem__("clock_screen", screen))


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
