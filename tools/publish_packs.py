#!/usr/bin/env python3
"""Stage the usage style packs for the GitHub Release `packs-api<N>` (what installed apps fetch).

Writes <out>/index.json ({"api", "rev", "styles": [{id, name, file, size, crc, light,
sha256}]}) and the packs it lists, from <src>/styles. The app accepts them only when the index
is signed by the release key (index.json.minisig, tools/publish_packs.sh signs it), is for the
style API its bundled firmware draws, and has a higher `rev` than the packs the app carries.

`rev` defaults to the commit time of the last change to <src>/styles: the same number
tools/bundle_firmware.py gives the bundled packs, so a release built from the same commit is
never "newer" than itself.

    python3 tools/publish_packs.py [--src clock/firmware] [--out build/packs] [--rev N]

Prints `api rev count` on success.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bundle_firmware import ROOT, SRC, copy_styles, style_api_from_source, styles_rev  # noqa: E402

DEST = ROOT / "build" / "packs"


def stage(src: Path, out: Path, rev: int | None = None) -> tuple[int, int, int]:
    """Write the release files into `out` (replacing it); (api, rev, count). ValueError when the
    packs do not check out or there are none."""
    api = style_api_from_source(src)
    if api < 1:
        raise ValueError(f"{src}: the firmware draws no style packs (STYLE_API 0)")
    rev = styles_rev(src) if rev is None else rev
    if rev < 1:
        raise ValueError("no rev: commit the style packs first (or pass --rev)")
    count = copy_styles(src, api, out, rev)
    if not count:
        raise ValueError(f"{src}/styles has no index.json")
    index = json.loads((out / "index.json").read_text())
    for e in index["styles"]:
        e["sha256"] = hashlib.sha256((out / e["file"]).read_bytes()).hexdigest()
    # compact and stable: the signature covers these exact bytes
    (out / "index.json").write_text(json.dumps(index, separators=(",", ":"), sort_keys=True))
    return api, rev, count


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--src", type=Path, default=SRC, help=f"the firmware source dir (default {SRC})")
    ap.add_argument("--out", type=Path, default=DEST, help=f"where to stage (default {DEST})")
    ap.add_argument("--rev", type=int, help="the packs' rev (default: commit time of <src>/styles)")
    a = ap.parse_args()
    try:
        api, rev, count = stage(a.src, a.out, a.rev)
    except (ValueError, OSError) as e:
        print(f"publish_packs: {e}", file=sys.stderr)
        return 1
    print(api, rev, count)
    return 0


if __name__ == "__main__":
    sys.exit(main())
