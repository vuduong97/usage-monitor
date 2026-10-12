"""The clock firmware that ships inside the app, and when to install it.

`tools/bundle_firmware.py` copies a clawd build (firmware.bin) into
`clawd_desktop/firmware/` with a `firmware.json` next to it:

    {"file": "clawd-firmware.bin", "size": 514528, "sha256": "...",
     "variant": "esp8266-clawd", "fwSeq": 1, "layouts": 3, "commit": "c2da6b4"}

fwSeq and layouts are what the image reports in `/api/heap` once it runs. The
app pushes it only to a clawd clock whose fwSeq is lower (missing = 0), so it
never downgrades one, and offers a usage layout either because the clock
already draws it or because the bundled image would.
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
from dataclasses import dataclass
from pathlib import Path

CLAWD_VARIANT = "esp8266-clawd"


def bundle_dir() -> Path:
    env = os.environ.get("CLAWD_FIRMWARE_DIR")
    if env:
        return Path(env)
    base = getattr(sys, "_MEIPASS", None)          # PyInstaller
    if base:
        return Path(base) / "clawd_desktop" / "firmware"
    return Path(__file__).resolve().parent / "firmware"


@dataclass(frozen=True)
class Bundle:
    path: Path
    size: int
    sha256: str
    variant: str
    seq: int
    layouts: int
    commit: str = ""

    def read(self) -> bytes:
        """The image, checked against the manifest (a damaged bundle is never sent)."""
        data = self.path.read_bytes()
        if len(data) != self.size or hashlib.sha256(data).hexdigest() != self.sha256:
            raise ValueError("bundled firmware does not match its manifest")
        return data


def load_bundle(folder: Path | None = None) -> Bundle | None:
    folder = folder or bundle_dir()
    try:
        m = json.loads((folder / "firmware.json").read_text())
        b = Bundle(path=folder / str(m["file"]), size=int(m["size"]), sha256=str(m["sha256"]),
                   variant=str(m["variant"]), seq=int(m["fwSeq"]), layouts=int(m["layouts"]),
                   commit=str(m.get("commit", "")))
    except (OSError, ValueError, KeyError, TypeError):
        return None
    return b if b.path.is_file() else None


def is_clawd(heap: dict) -> bool:
    """The clawd firmware: says so (`variant`), or predates that field but has the desk player."""
    v = heap.get("variant")
    if v is not None:
        return v == CLAWD_VARIANT
    return "desk" in heap and "pak" in heap


def clock_seq(heap: dict) -> int:
    s = heap.get("fwSeq")
    return s if isinstance(s, int) and not isinstance(s, bool) else 0


LAYOUT_COUNT = 6          # usage layouts the app knows (preview.js PV.LAYOUTS has the same list)


def clock_layouts(heap: dict) -> int:
    v = heap.get("layouts")
    return v if isinstance(v, int) and not isinstance(v, bool) else 1   # classic only


def update_state(heap: dict, bundle: Bundle | None) -> str:
    """"available" | "current" | "none" (no bundle) | "incompatible" (not a clawd clock)."""
    if bundle is None:
        return "none"
    if not is_clawd(heap) or bundle.variant != CLAWD_VARIANT:
        return "incompatible"
    return "available" if clock_seq(heap) < bundle.seq else "current"


def layout_state(index: int, heap: dict, bundle: Bundle | None) -> str:
    """"ready" (the clock draws it) | "update" (after installing the bundle) | "unavailable"."""
    if clock_layouts(heap) >> index & 1:
        return "ready"
    if bundle is not None and bundle.layouts >> index & 1 and update_state(heap, bundle) == "available":
        return "update"
    return "unavailable"
