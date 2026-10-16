#!/usr/bin/env python3
"""Lock interop partner for `clawd-store`'s ignored test `python_and_rust_share_the_prefs_lock`:
runs the REAL Python `paths.update_prefs` (the `desktop.lock` byte lock) in a loop against a
throwaway home, while the Rust test does the same from its side. Neither may lose a key.

    python tools/golden/store_lock_interop.py <home> <count> [<key-prefix>]

HOME / USERPROFILE are pointed at <home> before `clawd_desktop` is imported, so the real home is
never touched. Prints `done` when all updates succeeded.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path


def main() -> int:
    home, count = Path(sys.argv[1]).resolve(), int(sys.argv[2])
    prefix = sys.argv[3] if len(sys.argv) > 3 else "py"
    os.environ["HOME"] = os.environ["USERPROFILE"] = str(home)
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from clawd_desktop import paths
    if not str(paths.PREFS_FILE).startswith(str(home)):
        raise SystemExit(f"refusing: prefs file {paths.PREFS_FILE} is outside {home}")
    for i in range(count):
        paths.update_prefs(lambda p, i=i: p.__setitem__(f"{prefix}_{i}", i))
    print("done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
