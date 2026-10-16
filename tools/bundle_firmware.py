#!/usr/bin/env python3
"""Put a clawd firmware build inside the app (private/firmware/, git-ignored:
the firmware is private and never goes in this repo).

  python tools/bundle_firmware.py ../clawd-home/firmware/.pio/build/clawd/firmware.bin \
      --src ../clawd-home/firmware --commit c2da6b4
  CLAWD_PRIVATE_FIRMWARE=1 sh packaging-rs/build_mac.sh

fwSeq and layouts must be what that image reports in /api/heap: --src reads
them from the firmware source it was built from (CLAWD_FW_SEQ in config.h,
CLAWD_LAYOUTS in ClawdUi.h), or pass --seq/--layouts. A wrong seq would make
the app offer the same update again after installing it.

With --src it also copies the style packs built from that source (<src>/styles:
index.json + *.pak, from `node tools/style/build.js`) into styles/, and records
the style API the image draws (STYLE_API in StylePack.h).
"""
import argparse
import hashlib
import re
import json
import shutil
import sys
import zlib
from pathlib import Path

OTA_MAX = 524_288
DEST = Path(__file__).resolve().parent.parent / "private" / "firmware"


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


def style_api_from_source(src: Path) -> int:
    """STYLE_API from the firmware source, 0 for one from before style packs."""
    try:
        h = (src / "src" / "features" / "clawd" / "StylePack.h").read_text()
    except FileNotFoundError:
        return 0
    m = re.search(r"#define\s+STYLE_API\s+(\d+)", h)
    if not m:
        raise ValueError("no STYLE_API in StylePack.h")
    return int(m.group(1))


def copy_styles(src: Path, api: int, dest: Path) -> int:
    """Copy <src>/styles (index.json + the packs it lists) to `dest`; how many packs.
    ValueError when the index is not for `api` or a pack does not match it."""
    shutil.rmtree(dest, ignore_errors=True)
    folder = src / "styles"
    if not api or not (folder / "index.json").is_file():
        return 0
    index = json.loads((folder / "index.json").read_text())
    if index.get("api") != api:
        raise ValueError(f"styles/index.json is for style API {index.get('api')}, the image draws {api}")
    dest.mkdir(parents=True)
    for e in index["styles"]:
        data = (folder / e["file"]).read_bytes()
        if len(data) != e["size"] or data[:4] != b"CSTY" or f"{zlib.crc32(data[16:]):08x}" != e["crc"]:
            raise ValueError(f"styles/{e['file']} does not match index.json: rebuild the packs")
        (dest / e["file"]).write_bytes(data)
    shutil.copyfile(folder / "index.json", dest / "index.json")
    return len(index["styles"])


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
        try:
            style_api = style_api_from_source(a.src)
        except (OSError, ValueError) as e:
            print(f"--src: {e}")
            return 1
    else:
        style_api = 0
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
    try:
        styles = copy_styles(a.src, style_api, DEST / "styles") if a.src else 0
        if not a.src:
            shutil.rmtree(DEST / "styles", ignore_errors=True)
    except (OSError, ValueError, KeyError, TypeError) as e:
        print(f"styles: {e}")
        return 1
    manifest = {"file": "clawd-firmware.bin", "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                "variant": "esp8266-clawd", "fwSeq": a.seq, "layouts": a.layouts, "styleApi": style_api,
                "commit": a.commit}
    (DEST / "firmware.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"bundled {len(data):,} B fwSeq {a.seq} layouts {a.layouts} styleApi {style_api} "
          f"({styles} style packs) -> {DEST}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
