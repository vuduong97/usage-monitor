#!/usr/bin/env python3
"""Put the clock firmware build inside the app: build/firmware/ (git-ignored, rebuilt from
clock/firmware for every release; packaging/build_mac.sh and build_windows.ps1 ship it).

  (cd clock/firmware && pio run -e clawd)
  python3 tools/bundle_firmware.py                 # clock/firmware -> build/firmware
  sh packaging/build_mac.sh

tools/build_clock.sh runs all of it, plus the preview renderer (tools/build_preview.py).

The folder holds:
  firmware.json      file size sha256 variant fwSeq layouts styleApi commit, and
                     packs: {clawdpak|deskpak: {file, size, crc}}
  clawd-firmware.bin the image (<src>/.pio/build/clawd/firmware.bin unless given)
  clawd.pak desk.pak the animation packs the app sends to the clock (<src>/data/)
  styles/            the usage style packs (<src>/styles: index.json + *.pak, from
                     `node clock/firmware/tools/style/build.js`); index.json gains `rev`

fwSeq and layouts must be what that image reports in /api/heap, so they are read from the
source it was built from (CLAWD_FW_SEQ in config.h, CLAWD_LAYOUTS in ClawdUi.h); a wrong seq
would make the app offer the same update again after installing it. `rev` (the commit time of
the last change to <src>/styles, or --rev) orders the bundled packs against the remote ones
(tools/publish_packs.sh): the app takes remote packs only when their rev is higher.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import zlib
from pathlib import Path

OTA_MAX = 524_288
ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "clock" / "firmware"
DEST = ROOT / "build" / "firmware"
# kind -> (file under <src>/data, magic, accepted versions): what the clock's /api/clawdpak and
# /api/deskpak take.
PACKS = {"clawdpak": ("clawd.pak", b"CLWD", (1,)), "deskpak": ("desk.pak", b"CLDK", (2, 3))}


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


def git(src: Path, *args: str) -> str:
    """Output of `git -C src args`, "" when it fails (no git, not a checkout)."""
    try:
        r = subprocess.run(["git", "-C", str(src), *args], capture_output=True, text=True,
                           timeout=10, check=False)
    except (OSError, subprocess.SubprocessError):
        return ""
    return r.stdout.strip() if r.returncode == 0 else ""


def git_commit(src: Path) -> str:
    """The short commit the source is checked out at, "" outside git."""
    return git(src, "rev-parse", "--short", "HEAD")


def styles_rev(src: Path) -> int:
    """Commit time of the last change to <src>/styles: the packs' `rev`, 0 outside git."""
    out = git(src, "log", "-1", "--format=%ct", "--", "styles")
    return int(out) if out.isdigit() else 0


def copy_styles(src: Path, api: int, dest: Path, rev: int | None = None) -> int:
    """Copy <src>/styles (index.json + the packs it lists) to `dest`; how many packs. With `rev`,
    index.json gets that `rev`. ValueError when the index is not for `api` or a pack does not
    match it."""
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
    if rev is None:
        shutil.copyfile(folder / "index.json", dest / "index.json")
    else:
        index["rev"] = rev
        (dest / "index.json").write_text(json.dumps(index, indent=2) + "\n")
    return len(index["styles"])


def check_pack(kind: str, data: bytes) -> None:
    """ValueError unless `data` is a pack of that kind the clock takes."""
    name, magic, versions = PACKS[kind]
    if len(data) < 6 or data[:4] != magic or int.from_bytes(data[4:6], "little") not in versions:
        raise ValueError(f"{name}: not a {magic.decode()} pack of version {'/'.join(map(str, versions))}")


def bundle(src: Path, image: Path, dest: Path, commit: str, rev: int) -> dict:
    """Write the bundle into `dest` (replacing it); its firmware.json. ValueError/OSError when an
    input does not check out."""
    seq, layouts = from_source(src)
    style_api = style_api_from_source(src)
    data = image.read_bytes()
    if not data or data[0] != 0xE9:
        raise ValueError(f"{image}: not an ESP8266 app image")
    if len(data) > OTA_MAX:
        raise ValueError(f"{image}: {len(data):,} B is over the {OTA_MAX:,} B OTA budget")
    if seq < 1 or not 1 <= layouts < 1 << 16 or not layouts & 1:
        raise ValueError("fwSeq must be >= 1 and the layouts must include classic (bit 0)")
    packs = {}
    for kind, (name, _, _) in PACKS.items():
        p = (src / "data" / name).read_bytes()
        check_pack(kind, p)
        packs[kind] = (name, p)
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    (dest / "clawd-firmware.bin").write_bytes(data)
    for name, p in packs.values():
        (dest / name).write_bytes(p)
    copy_styles(src, style_api, dest / "styles", rev)
    manifest = {"file": "clawd-firmware.bin", "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                "variant": "esp8266-clawd", "fwSeq": seq, "layouts": layouts, "styleApi": style_api,
                "commit": commit,
                "packs": {kind: {"file": name, "size": len(p), "crc": f"{zlib.crc32(p):08x}"}
                          for kind, (name, p) in packs.items()}}
    (dest / "firmware.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def check(folder: Path, src: Path) -> dict:
    """ValueError unless `folder` is a whole bundle (what the app reads, every file intact) built
    from this source's fwSeq and style API: a stale or damaged artifact never ships."""
    m = json.loads((folder / "firmware.json").read_text())
    data = (folder / m["file"]).read_bytes()
    if len(data) != m["size"] or hashlib.sha256(data).hexdigest() != m["sha256"] or data[:1] != b"\xe9":
        raise ValueError(f"{m['file']} does not match firmware.json")
    seq, layouts = from_source(src)
    if (m["fwSeq"], m["layouts"], m["styleApi"]) != (seq, layouts, style_api_from_source(src)):
        raise ValueError(f"firmware.json is fwSeq {m['fwSeq']} / styleApi {m['styleApi']}, the source says "
                         f"{seq} / {style_api_from_source(src)}: rebuild it")
    for kind, (name, _, _) in PACKS.items():
        e = m["packs"][kind]
        p = (folder / e["file"]).read_bytes()
        check_pack(kind, p)
        if len(p) != e["size"] or f"{zlib.crc32(p):08x}" != e["crc"]:
            raise ValueError(f"{e['file']} does not match firmware.json")
    if m["styleApi"]:
        index = json.loads((folder / "styles" / "index.json").read_text())
        if index.get("api") != m["styleApi"] or not index.get("styles"):
            raise ValueError("styles/index.json is not for the image's style API")
        for e in index["styles"]:
            d = (folder / "styles" / e["file"]).read_bytes()
            if len(d) != e["size"] or d[:4] != b"CSTY" or f"{zlib.crc32(d[16:]):08x}" != e["crc"]:
                raise ValueError(f"styles/{e['file']} does not match index.json")
    return m


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("image", type=Path, nargs="?",
                    help="the firmware image (default <src>/.pio/build/clawd/firmware.bin)")
    ap.add_argument("--src", type=Path, default=SRC, help=f"the firmware source dir (default {SRC})")
    ap.add_argument("--out", type=Path, default=DEST, help=f"the bundle folder (default {DEST})")
    ap.add_argument("--commit", default="", help="firmware commit (default: git HEAD of --src)")
    ap.add_argument("--rev", type=int, help="the style packs' rev (default: commit time of <src>/styles)")
    ap.add_argument("--clear", action="store_true", help="remove the bundle instead")
    ap.add_argument("--check", type=Path, metavar="DIR",
                    help="only check the bundle in DIR against itself and --src (packaging does this)")
    a = ap.parse_args()
    if a.check:
        try:
            m = check(a.check, a.src)
        except (OSError, ValueError, KeyError, TypeError) as e:
            print(f"bundle_firmware: {a.check}: {e}", file=sys.stderr)
            return 1
        print(f"{a.check}: fwSeq {m['fwSeq']} styleApi {m['styleApi']} commit {m['commit']} OK")
        return 0
    if a.clear:
        shutil.rmtree(a.out, ignore_errors=True)
        print("bundled firmware removed")
        return 0
    image = a.image or a.src / ".pio" / "build" / "clawd" / "firmware.bin"
    rev = a.rev if a.rev is not None else styles_rev(a.src)
    try:
        m = bundle(a.src, image, a.out, a.commit or git_commit(a.src), rev)
    except (OSError, ValueError, KeyError, TypeError) as e:
        print(f"bundle_firmware: {e}")
        return 1
    styles = len(list((a.out / "styles").glob("*.pak")))
    print(f"bundled {m['size']:,} B fwSeq {m['fwSeq']} layouts {m['layouts']} styleApi {m['styleApi']} "
          f"commit {m['commit'] or '?'} ({styles} style packs, rev {rev}) -> {a.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
