#!/usr/bin/env python3
"""Fail when the repo tracks files that must stay private: clock firmware,
animation packs, fonts, or the Clawd/Anthropic artwork. Run in CI on every push."""
import re
import subprocess
import sys

BANNED = [
    (re.compile(r"\.(bin|pak|elf|ttf|otf|woff2?|bdf|pcf|gif|apng)$", re.I), "firmware/pack/font/animation file"),
    (re.compile(r"(^|/)clawd_fonts?(/|$)", re.I), "Clawd fonts"),
    (re.compile(r"(^|/)(clawd-on-desk|desk_assets|assets/clawd)(/|$)", re.I), "Clawd artwork"),
    (re.compile(r"(^|/)\.stock-backup(/|$)"), "stock clock backup"),
    (re.compile(r"(^|/)(firmware)(/|$)", re.I), "firmware source"),
    (re.compile(r"(^|/)clawd_render(\.exe)?$|\.zip$", re.I), "compiled clock-screen renderer (feed only)"),
]
MAX_BYTES = 512_000   # nothing in this repo should be big; a large blob is probably art


def main() -> int:
    files = subprocess.run(["git", "ls-files", "-z"], capture_output=True, check=True).stdout.decode().split("\0")
    bad = []
    for f in filter(None, files):
        for rx, why in BANNED:
            if rx.search(f):
                bad.append(f"{f}: {why}")
        try:
            size = subprocess.run(["git", "cat-file", "-s", f":{f}"], capture_output=True, text=True).stdout.strip()
            if size and int(size) > MAX_BYTES and f != "uv.lock":
                bad.append(f"{f}: {int(size):,} bytes (over {MAX_BYTES:,})")
        except ValueError:
            pass
    if bad:
        print("Private files must not be in the public repo:\n  " + "\n  ".join(bad))
        return 1
    print(f"check_public: {len([f for f in files if f])} files ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
