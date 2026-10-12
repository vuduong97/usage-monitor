"""What the clock's screen looks like, drawn by the clock's own code.

`tools/build_preview.py` compiles `clawd_render` from the firmware's drawing
sources into `clawd_desktop/renderer/` (git-ignored, private like the bundled
firmware). Given the usage payload the app last sent the clock and a layout it
writes the 240x240 frame the clock would show. This module runs it and wraps
the pixels in a BMP the settings page can show.

The renderer is only trusted when it was built from the firmware that ships in
the app (same commit): otherwise the page falls back to its own approximate
canvas, which it labels as such.
"""
from __future__ import annotations

import json
import os
import struct
import subprocess
import sys
import time
from base64 import b64encode
from collections import OrderedDict
from dataclasses import dataclass
from pathlib import Path

W = H = 240
FRAME_BYTES = W * H * 3
RUN_TIMEOUT = 15          # seconds; a frame takes well under one
CACHE_MAX = 32


def preview_dir() -> Path:
    env = os.environ.get("CLAWD_PREVIEW_DIR")
    if env:
        return Path(env)
    base = getattr(sys, "_MEIPASS", None)          # PyInstaller
    if base:
        return Path(base) / "clawd_desktop" / "renderer"
    return Path(__file__).resolve().parent / "renderer"


@dataclass(frozen=True)
class Renderer:
    exe: Path
    pak: Path
    commit: str
    layouts: int

    def draw(self, layout: int, payload: dict | None) -> bytes:
        """RGB bytes of the clock's screen for `layout` with this payload (None or a
        payload the clock would reject: its waiting screen). Raises OSError/ValueError."""
        stdin = json.dumps(payload).encode() if payload else b""
        extra = {"creationflags": subprocess.CREATE_NO_WINDOW} if sys.platform == "win32" else {}
        r = subprocess.run([str(self.exe), "--layout", str(layout), "--pak", str(self.pak)],
                           input=stdin, capture_output=True, timeout=RUN_TIMEOUT, check=False, **extra)
        if r.returncode == 2 and payload:      # the clock ignores such a payload: it would stay on its last screen
            return self.draw(layout, None)
        if r.returncode != 0 or len(r.stdout) != FRAME_BYTES:
            raise ValueError(f"clawd_render failed ({r.returncode}): {r.stderr[-200:].decode(errors='replace')}")
        return r.stdout


def load_renderer(folder: Path | None = None, firmware_commit: str = "") -> Renderer | None:
    """The built renderer, or None when it is missing, damaged or built from another firmware
    than the one the app ships (`firmware_commit`, "" = no bundled firmware to compare)."""
    folder = folder or preview_dir()
    try:
        m = json.loads((folder / "preview.json").read_text())
        r = Renderer(exe=folder / str(m["exe"]), pak=folder / str(m["pak"]),
                     commit=str(m.get("commit", "")), layouts=int(m["layouts"]))
    except (OSError, ValueError, KeyError, TypeError):
        return None
    if not r.exe.is_file() or not r.pak.is_file():
        return None
    if firmware_commit and r.commit and r.commit != firmware_commit:
        return None
    return r


def bmp_data_url(rgb: bytes) -> str:
    """A 24-bit BMP of a W x H RGB frame as a data: URL (both webviews show BMP)."""
    if len(rgb) != FRAME_BYTES:
        raise ValueError("not a 240x240 RGB frame")
    stride = W * 3                                   # 720: already a multiple of 4
    rows = []
    for y in range(H - 1, -1, -1):                   # bottom-up
        row = bytearray(rgb[y * stride:(y + 1) * stride])
        row[0::3], row[2::3] = row[2::3], row[0::3]  # RGB -> BGR
        rows.append(bytes(row))
    pixels = b"".join(rows)
    head = b"BM" + struct.pack("<IHHI", 54 + len(pixels), 0, 0, 54)
    head += struct.pack("<IiiHHIIiiII", 40, W, H, 1, 24, 0, len(pixels), 2835, 2835, 0, 0)
    return "data:image/bmp;base64," + b64encode(head + pixels).decode()


def live_payload(payload: dict | None, now: float) -> dict | None:
    """The stored payload with its wall-clock time moved to `now` (the title clock of the
    clock ticks on): `t` is local wall-clock seconds, as usage.add_clock_fields makes it."""
    if not payload:
        return None
    out = dict(payload)
    if "t" in out:
        out["t"] = int(now) + time.localtime(now).tm_gmtoff
    return out


class FrameCache:
    """The last few frames, so reopening a tab or polling does not run the renderer again."""

    def __init__(self, size: int = CACHE_MAX) -> None:
        self._d: OrderedDict[str, str] = OrderedDict()
        self._size = size

    def get(self, key: str) -> str | None:
        v = self._d.get(key)
        if v is not None:
            self._d.move_to_end(key)
        return v

    def put(self, key: str, value: str) -> None:
        self._d[key] = value
        self._d.move_to_end(key)
        while len(self._d) > self._size:
            self._d.popitem(last=False)


def frame_key(layout: int, payload: dict | None, minute: int) -> str:
    # the title clock moves once a minute, so a frame is good for that long
    return f"{layout}|{minute}|{json.dumps(payload, sort_keys=True)}"
