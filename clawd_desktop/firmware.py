"""The clock firmware that ships inside the app, and when to install it.

`tools/bundle_firmware.py` copies a clawd build (firmware.bin) into
`clawd_desktop/firmware/` with a `firmware.json` next to it:

    {"file": "clawd-firmware.bin", "size": 514528, "sha256": "...",
     "variant": "esp8266-clawd", "fwSeq": 4, "layouts": 1, "styleApi": 1, "commit": "c2da6b4"}

fwSeq, layouts and styleApi are what the image reports in `/api/heap` once it runs.
The app pushes it only to a clawd clock whose fwSeq is lower (missing = 0), so it
never downgrades one.

Next to it, `styles/` holds the style packs built with that firmware (index.json +
<id>.pak, see the firmware's docs/STYLE_PACK.md). A pack is sent to the clock
(`POST /api/style`) only when the clock draws its API, either already or once the
bundled image is installed.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import sys
import zlib
from dataclasses import dataclass, field, replace
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
class StylePack:
    """One style pack from the bundle's styles/index.json."""
    id: str
    name: str
    path: Path
    size: int
    crc: int                                # the pack header's CRC-32 of bytes [16..)
    light: bool = False
    data: bytes | None = field(default=None, repr=False, compare=False)   # in memory (feed.py)

    def read(self) -> bytes:
        """The pack, checked against the index and its own header (a damaged one is never sent)."""
        data = self.data if self.data is not None else self.path.read_bytes()
        if (len(data) != self.size or data[:4] != STYLE_MAGIC
                or int.from_bytes(data[12:16], "little") != self.crc or zlib.crc32(data[16:]) != self.crc):
            raise ValueError(f"style pack {self.id} does not match the index")
        return data


STYLE_MAGIC = b"CSTY"
STYLE_MAX = 20_480                          # the pack compiler's cap
STYLE_ID = re.compile(r"[a-z0-9_-]{1,15}")


def load_styles(folder: Path, api: int) -> tuple[StylePack, ...]:
    """The packs in `folder`/index.json built for style API `api`; () when none or unreadable."""
    try:
        m = json.loads((folder / "index.json").read_text())
        if int(m["api"]) != api or api < 1:
            return ()
        out = []
        for e in m["styles"]:
            sid, f = str(e["id"]), str(e["file"])
            if not STYLE_ID.fullmatch(sid) or "/" in f or "\\" in f or not 0 < int(e["size"]) <= STYLE_MAX:
                return ()
            out.append(StylePack(id=sid, name=str(e["name"])[:23], path=folder / f, size=int(e["size"]),
                                 crc=int(str(e["crc"]), 16), light=bool(e.get("light"))))
    except (OSError, ValueError, KeyError, TypeError):
        return ()
    if len({p.id for p in out}) != len(out) or not all(p.path.is_file() for p in out):
        return ()
    return tuple(out)


@dataclass(frozen=True)
class Bundle:
    path: Path
    size: int
    sha256: str
    variant: str
    seq: int
    layouts: int
    commit: str = ""
    style_api: int = 0                      # style packs the image draws (0: none)
    styles: tuple[StylePack, ...] = ()

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
                   commit=str(m.get("commit", "")), style_api=int(m.get("styleApi", 0)))
        b = replace(b, styles=load_styles(folder / "styles", b.style_api))
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


def update_state(heap: dict, bundle: Bundle | None) -> str:
    """"available" | "current" | "none" (no bundle) | "incompatible" (not a clawd clock)."""
    if bundle is None:
        return "none"
    if not is_clawd(heap) or bundle.variant != CLAWD_VARIANT:
        return "incompatible"
    return "available" if clock_seq(heap) < bundle.seq else "current"


def clock_style_api(heap: dict) -> int:
    """The style pack API the clock draws (0: an image from before style packs)."""
    v = heap.get("styleApi")
    return v if isinstance(v, int) and not isinstance(v, bool) and v > 0 else 0


def clock_style(heap: dict) -> str:
    """The id of the pack the clock shows, "" for the classic screen."""
    v = heap.get("style") if clock_style_api(heap) else ""
    return v if isinstance(v, str) else ""


def style_state(heap: dict, bundle: Bundle | None) -> str:
    """For the bundled packs: "ready" (the clock draws them) | "update" (after installing
    the bundled image) | "unavailable"."""
    if bundle is None or not bundle.styles:
        return "unavailable"
    if clock_style_api(heap) == bundle.style_api:
        return "ready"
    if update_state(heap, bundle) == "available":
        return "update"
    return "unavailable"
