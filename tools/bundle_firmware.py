#!/usr/bin/env python3
"""Put a clawd firmware build inside the app (clawd_desktop/firmware/, git-ignored:
the firmware is private and never goes in this repo).

  python tools/bundle_firmware.py ../clawd-home/smalltv/.pio/build/smalltv_clawd/firmware.bin \
      --seq 1 --layouts 3 --commit c2da6b4

--seq and --layouts must be what that image reports in /api/heap (`fwSeq`,
`layouts`; CLAWD_FW_SEQ in config.h, CLAWD_LAYOUTS in ClawdUi.h). The app
offers the update to clawd clocks with a lower fwSeq.
"""
import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

OTA_MAX = 524_288
DEST = Path(__file__).resolve().parent.parent / "clawd_desktop" / "firmware"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("image", type=Path)
    ap.add_argument("--seq", type=int, required=True)
    ap.add_argument("--layouts", type=int, required=True)
    ap.add_argument("--commit", default="")
    ap.add_argument("--clear", action="store_true", help="remove the bundled firmware instead")
    a = ap.parse_args()
    if a.clear:
        shutil.rmtree(DEST, ignore_errors=True)
        print("bundled firmware removed")
        return 0
    data = a.image.read_bytes()
    if not data or data[0] != 0xE9:
        print(f"{a.image}: not an ESP8266 app image")
        return 1
    if len(data) > OTA_MAX:
        print(f"{a.image}: {len(data):,} B is over the {OTA_MAX:,} B OTA budget")
        return 1
    if a.seq < 1 or not 1 <= a.layouts < 1 << 16 or not a.layouts & 1:
        print("--seq must be >= 1 and --layouts must include classic (bit 0)")
        return 1
    DEST.mkdir(parents=True, exist_ok=True)
    for old in DEST.glob("*.bin"):
        old.unlink()
    (DEST / "clawd-firmware.bin").write_bytes(data)
    manifest = {"file": "clawd-firmware.bin", "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                "variant": "esp8266-clawd", "fwSeq": a.seq, "layouts": a.layouts, "commit": a.commit}
    (DEST / "firmware.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"bundled {len(data):,} B fwSeq {a.seq} layouts {a.layouts} -> {DEST}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
