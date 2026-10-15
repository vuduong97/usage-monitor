#!/usr/bin/env python3
"""Sample exporter (phase 02): proves the harness pipeline end to end by writing
`desktop-rs/fixtures/store/prefs_roundtrip.json`. Phase 03 replaces it with `export_store.py`.

Runs the real `paths.update_prefs` on a prefs file that holds unknown keys, a float, unicode and
wrongly typed values (`usage.screen = 300`, `setup_done` not a list), changes ONE key, and records
the exact file text afterwards: nothing but that key may change.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
from clawd_desktop import paths  # noqa: E402  (after isolate())

INITIAL = {
    "setup_done": "clock",              # wrong type: must survive untouched
    "usage": {"screen": 300},           # out-of-range value
    "future_key": {"nested": [1, 2.5, None, True]},
    "ratio": 1.0,                       # Python writes 1.0, not 1
    "name": "Đồng hồ \u2603",           # non-ASCII: json.dumps escapes it (ensure_ascii)
    "clock_screen": 0,
}

paths.CONFIG_DIR.mkdir(parents=True, exist_ok=True)
paths.PREFS_FILE.write_text(json.dumps(INITIAL, indent=2))
result = paths.update_prefs(lambda prefs: prefs.__setitem__("clock_screen", 1))
text = paths.PREFS_FILE.read_text()

out = h.write_fixture(
    "store", "prefs_roundtrip",
    input={"initial": INITIAL, "change": {"set": {"clock_screen": 1}}},
    output={"prefs": result, "file_text": text, "clock_screen": paths.clock_screen()},
    ctx=ctx,
)
print(f"wrote {out.relative_to(h.REPO)}")
