"""Hold the engine's pushes while the settings window works on the clock.

The ESP8266 serves one HTTP client at a time. During a firmware upload and
the reboot after it, the engine's usage/state pushes and restart probes would
compete with the upload, then fail and get the clock dropped from discovery.
The settings window (a separate process) writes `pause.json` with an `until`
time (wall clock, shared by both processes); while it is in the future the
engine's target list is empty, so nothing is sent. It is a lease: the window
renews it while it waits and removes it when done, and a crashed window's
pause simply runs out.
"""
from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Callable

from clawd_desktop import paths

PAUSE_FILE = paths.CONFIG_DIR / "pause.json"
MAX_LEASE = 300.0       # never trust an `until` further out than this


def hold(seconds: float, reason: str, path: Path = PAUSE_FILE,
         now: Callable[[], float] = time.time) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    tmp.write_text(json.dumps({"until": now() + min(seconds, MAX_LEASE), "reason": reason,
                               "pid": os.getpid()}))
    tmp.replace(path)


def release(path: Path = PAUSE_FILE) -> None:
    try:
        path.unlink()
    except FileNotFoundError:
        pass


class PauseReader:
    """For the engine: `reason()` is the active pause's reason, or None. Re-reads
    the file only when it changes (it is asked every second)."""

    def __init__(self, path: Path = PAUSE_FILE, now: Callable[[], float] = time.time) -> None:
        self.path, self.now = path, now
        self._stamp: tuple | None = None
        self._until = 0.0
        self._reason = ""

    def reason(self) -> str | None:
        try:
            st = self.path.stat()
        except OSError:
            self._stamp = None
            return None
        stamp = (st.st_mtime_ns, st.st_ino, st.st_size)   # a rewrite is a new file (replace)
        if stamp != self._stamp:
            self._stamp = stamp
            try:
                d = json.loads(self.path.read_text())
                self._until = float(d.get("until", 0))
                self._reason = str(d.get("reason") or "settings")
            except (OSError, ValueError, TypeError, AttributeError):
                self._until, self._reason = 0.0, ""
        t = self.now()
        if self._until <= t or self._until - t > MAX_LEASE:
            return None
        return self._reason
