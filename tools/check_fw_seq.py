#!/usr/bin/env python3
"""CI guard: a change to the clock firmware's code must raise CLAWD_FW_SEQ.

The app installs its bundled image only on a clock whose fwSeq is lower (firmware.update_state),
so new firmware code under the old number would never reach a clock that has the old image.

    python3 tools/check_fw_seq.py <base-commit>

Compares <base-commit> with HEAD. No base (a new branch, the zero SHA) or a base the checkout
does not have: nothing to compare, passes. Style packs (clock/firmware/styles) and the animation
packs are not code: they need no bump.
"""

from __future__ import annotations

import re
import subprocess
import sys

CODE = "clock/firmware/src"
CONFIG = f"{CODE}/config.h"


def git(*args: str) -> tuple[int, str]:
    r = subprocess.run(["git", *args], capture_output=True, text=True, check=False)
    return r.returncode, r.stdout


def seq_of(text: str) -> int:
    m = re.search(r"#define\s+CLAWD_FW_SEQ\s+(\d+)", text)
    return int(m.group(1)) if m else 0


def main() -> int:
    base = sys.argv[1] if len(sys.argv) > 1 else ""
    if not base or set(base) == {"0"} or git("cat-file", "-e", f"{base}^{{commit}}")[0] != 0:
        print(f"check_fw_seq: no base to compare with ({base or 'none'}): skipped")
        return 0
    _, changed = git("diff", "--name-only", base, "HEAD", "--", CODE)
    changed = [f for f in changed.split() if f]
    if not changed:
        print("check_fw_seq: no firmware code changed")
        return 0
    code, old = git("show", f"{base}:{CONFIG}")
    before = seq_of(old) if code == 0 else 0
    _, new = git("show", f"HEAD:{CONFIG}")
    after = seq_of(new)
    if after > before:
        print(f"check_fw_seq: firmware code changed, CLAWD_FW_SEQ {before} -> {after}: OK")
        return 0
    print(f"::error::firmware code changed ({len(changed)} files under {CODE}, e.g. {changed[0]}) "
          f"but CLAWD_FW_SEQ stayed {after} (was {before}): raise it in {CONFIG}, or no installed "
          "app will put the new image on a clock that has the old one")
    return 1


if __name__ == "__main__":
    sys.exit(main())
