#!/usr/bin/env python3
"""Put a clawd firmware build inside the app (clawd_desktop/firmware/, git-ignored:
the firmware is private and never goes in this repo).

  python tools/bundle_firmware.py ../clawd-home/firmware/.pio/build/clawd/firmware.bin \
      --src ../clawd-home/firmware --commit c2da6b4
  CLAWD_PRIVATE_FIRMWARE=1 sh packaging/build_mac.sh

fwSeq and layouts must be what that image reports in /api/heap: --src reads
them from the firmware source it was built from (CLAWD_FW_SEQ in config.h,
CLAWD_LAYOUTS in ClawdUi.h), or pass --seq/--layouts. A wrong seq would make
the app offer the same update again after installing it.
"""
import argparse
import hashlib
import re
import json
import shutil
import sys
from pathlib import Path

OTA_MAX = 524_288
DEST = Path(__file__).resolve().parent.parent / "clawd_desktop" / "firmware"


def from_source(src: Path) -> tuple[int, int]:
    """(CLAWD_FW_SEQ, CLAWD_LAYOUTS) from the firmware source."""
    cfg = (src / "src" / "config.h").read_text()
    m = re.search(r"#define\s+CLAWD_FW_SEQ\s+(\d+)", cfg)
    if not m:
        raise ValueError("no CLAWD_FW_SEQ in src/config.h")
    ui = (src / "src" / "features" / "clawd" / "ClawdUi.h").read_text()
    ui = re.sub(r"\\\r?\n", " ", ui)          # a #define may continue over several lines
    enum = {k: int(v) for k, v in re.findall(r"(CLAWD_LAYOUT_[A-Z]+)\s*=\s*(\d+)", ui)}
    d = re.search(r"#define\s+CLAWD_LAYOUTS\s+(.+)", ui)
    if not d or not enum:
        raise ValueError("no CLAWD_LAYOUTS in ClawdUi.h")
    names = re.findall(r"1u?\s*<<\s*(CLAWD_LAYOUT_[A-Z]+)", d.group(1))
    if not names or any(n not in enum for n in names):
        raise ValueError(f"cannot read CLAWD_LAYOUTS: {d.group(1).strip()}")
    return int(m.group(1)), sum(1 << enum[n] for n in set(names))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("image", type=Path)
    ap.add_argument("--src", type=Path, help="the firmware source dir the image was built from")
    ap.add_argument("--seq", type=int)
    ap.add_argument("--layouts", type=int)
    ap.add_argument("--commit", default="")
    ap.add_argument("--clear", action="store_true", help="remove the bundled firmware instead")
    a = ap.parse_args()
    if a.clear:
        shutil.rmtree(DEST, ignore_errors=True)
        print("bundled firmware removed")
        return 0
    if a.src:
        try:
            seq, layouts = from_source(a.src)
        except (OSError, ValueError) as e:
            print(f"--src: {e}")
            return 1
        if (a.seq is not None and a.seq != seq) or (a.layouts is not None and a.layouts != layouts):
            print(f"--seq/--layouts disagree with the source (fwSeq {seq}, layouts {layouts})")
            return 1
        a.seq, a.layouts = seq, layouts
    if a.seq is None or a.layouts is None:
        print("give --src, or both --seq and --layouts")
        return 1
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
