"""Run Clawd from source and restart it when a .py file changes (`make dev`).

  python tools/dev.py settings   the settings window (default)
  python tools/dev.py tray       the tray app

CLAWD_DEV=1 is set for the child, so the settings window also reloads its page
by itself when a file under clawd_desktop/ui/ changes (no restart needed).
"""
from __future__ import annotations

import contextlib
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "clawd_desktop"


def log(msg: str) -> None:
    print(f"\033[2m[{time.strftime('%H:%M:%S')}] [dev]\033[0m {msg}", flush=True)


def stamp() -> dict[Path, int]:
    out = {}
    for p in SRC.rglob("*.py"):
        with contextlib.suppress(OSError):  # an editor's atomic save: gone for a moment
            out[p] = p.stat().st_mtime_ns
    return out


def start(cmd: list[str]) -> subprocess.Popen:
    env = dict(os.environ, CLAWD_DEV="1", PYTHONUNBUFFERED="1")
    # Its own process group: the tray's settings window goes down with it.
    return subprocess.Popen(cmd, cwd=ROOT, env=env, start_new_session=hasattr(os, "killpg"))


def stop(p: subprocess.Popen) -> None:
    if p.poll() is not None:
        return
    try:
        if hasattr(os, "killpg"):
            os.killpg(p.pid, signal.SIGTERM)
        else:
            p.terminate()
        p.wait(timeout=3)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
    except ProcessLookupError:              # already gone between poll() and the signal
        return


def other_clawd() -> bool:
    """Is the installed Clawd.app running? Two trays would both drive the clock."""
    if sys.platform != "darwin":
        return False
    r = subprocess.run(["pgrep", "-x", "Clawd"], capture_output=True)
    return r.returncode == 0


def main(argv: list[str]) -> int:
    what = argv[0] if argv else "settings"
    if what not in ("settings", "tray"):
        print(__doc__)
        return 2
    if what == "tray" and other_clawd():
        log("\033[33mClawd.app is running: quit it first (pkill -x Clawd), or both will drive the clock\033[0m")
    def quit_(*_):                          # kill / closed terminal: stop the child too, like Ctrl-C
        raise KeyboardInterrupt
    for sig in ("SIGTERM", "SIGHUP"):
        if hasattr(signal, sig):
            signal.signal(getattr(signal, sig), quit_)
    cmd = [sys.executable, "-m", "clawd_desktop", what]
    log(f"{what} from source · .py change = restart" + (" · ui/ change = reload" if what == "settings" else "")
        + " · Ctrl-C to stop")
    last, p, exited = stamp(), start(cmd), False
    try:
        while True:
            time.sleep(0.4)
            now = stamp()
            if now != last:
                changed = sorted({k.relative_to(ROOT).as_posix() for k in now.keys() ^ last.keys()}
                                 | {k.relative_to(ROOT).as_posix() for k in now.keys() & last.keys()
                                    if now[k] != last[k]})
                last = now
                log("changed " + ", ".join(changed[:3]) + (" …" if len(changed) > 3 else "") + ": restart")
                stop(p)
                p, exited = start(cmd), False
            elif p.poll() is not None and not exited:
                exited = True
                log(f"{what} exited ({p.returncode}); waiting for a change (Ctrl-C to stop)")
    except KeyboardInterrupt:
        pass
    finally:
        stop(p)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
