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
# Files the owner reviewed and chose to keep despite their size (exact paths, nothing else).
SIZE_ALLOWED = {"Clawd Print & Paper.html"}

# Golden fixtures of the Rust port (fixtures/**) come from running the real app, so
# they could leak art, secrets or the author's home path. Scanned as text; binary content fails.
FIXTURE_DIR = "fixtures/"
FIXTURE_PATTERNS = [
    (re.compile(rb"CLWD"), "clawd.pak magic (animation pack)"),
    (re.compile(rb"iVBORw0KGgo"), "base64 PNG/APNG (art)"),
    # real tokens look like sk-ant-oat01-... / sk-ant-api03-...; sk-ant-test-/fake- are fixtures
    (re.compile(rb"sk-ant-(?!test-|fake-)"), "Anthropic token"),
    (re.compile(rb"/Users/"), "absolute macOS home path"),
    (re.compile(rb"[A-Za-z]:\\{1,2}Users\\", re.I), "absolute Windows home path"),
]


def check_fixture(path: str) -> list[str]:
    try:
        with open(path, "rb") as fh:
            data = fh.read()
    except OSError:
        return []
    bad = [f"{path}: {why}" for rx, why in FIXTURE_PATTERNS if rx.search(data)]
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        bad.append(f"{path}: not UTF-8 text (fixtures store hashes, never binary)")
    return bad


def main() -> int:
    files = subprocess.run(["git", "ls-files", "-z"], capture_output=True, check=True).stdout.decode().split("\0")
    bad = []
    for f in filter(None, files):
        for rx, why in BANNED:
            if rx.search(f):
                bad.append(f"{f}: {why}")
        if f.startswith(FIXTURE_DIR):
            bad.extend(check_fixture(f))
        try:
            size = subprocess.run(["git", "cat-file", "-s", f":{f}"], capture_output=True, text=True).stdout.strip()
            if size and int(size) > MAX_BYTES and f not in SIZE_ALLOWED:
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
