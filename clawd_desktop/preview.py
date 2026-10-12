"""What the clock's screen looks like, drawn by the clock's own code.

`tools/build_preview.py` compiles `clawd_render` from the firmware's drawing
sources into `clawd_desktop/renderer/` (git-ignored, private like the bundled
firmware). Given the usage payload the app last sent the clock and a layout it
writes the 240x240 frame the clock would show, or (`--clip`) one loop of the
screen as it animates. This module runs it and wraps the pixels in a BMP, or the
loop in an animated PNG, that the settings page can show.

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
import zlib
from base64 import b64encode
from collections import OrderedDict
from dataclasses import dataclass
from pathlib import Path

W = H = 240
FRAME_BYTES = W * H * 3
RUN_TIMEOUT = 15          # seconds; a frame takes well under one, a clip about one
CACHE_MAX = 32
CLIP_MAGIC = b"CLP1"


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

    def clip(self, layout: int, payload: dict | None) -> list[Frame]:
        """One loop of the screen as it animates (see parse_clip). Raises OSError/ValueError."""
        stdin = json.dumps(payload).encode() if payload else b""
        extra = {"creationflags": subprocess.CREATE_NO_WINDOW} if sys.platform == "win32" else {}
        r = subprocess.run([str(self.exe), "--layout", str(layout), "--pak", str(self.pak), "--clip"],
                           input=stdin, capture_output=True, timeout=RUN_TIMEOUT, check=False, **extra)
        if r.returncode == 2 and payload:
            return self.clip(layout, None)
        if r.returncode != 0:
            raise ValueError(f"clawd_render --clip failed ({r.returncode}): {r.stderr[-200:].decode(errors='replace')}")
        return parse_clip(r.stdout)


@dataclass(frozen=True)
class Frame:
    """One step of a clip: `rgb` (w*h*3 bytes) painted at x, y, then shown for `delay_ms`."""
    delay_ms: int
    x: int
    y: int
    w: int
    h: int
    rgb: bytes


def parse_clip(data: bytes) -> list[Frame]:
    """clawd_render --clip output: b"CLP1", u16 count, then per frame u16 delay x y w h and the
    RGB rectangle (little-endian). Frame 0 must be the whole screen."""
    if data[:4] != CLIP_MAGIC or len(data) < 6:
        raise ValueError("not a clawd_render clip")
    (count,) = struct.unpack_from("<H", data, 4)
    off, frames = 6, []
    for _ in range(count):
        if off + 10 > len(data):
            raise ValueError("clip cut short")
        delay, x, y, w, h = struct.unpack_from("<5H", data, off)
        off += 10
        n = w * h * 3
        if not w or not h or x + w > W or y + h > H or off + n > len(data):
            raise ValueError("bad clip frame")
        frames.append(Frame(delay, x, y, w, h, data[off:off + n]))
        off += n
    if off != len(data) or not frames or (frames[0].x, frames[0].y, frames[0].w, frames[0].h) != (0, 0, W, H):
        raise ValueError("bad clip")
    return frames


def _chunk(kind: bytes, body: bytes) -> bytes:
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))


def _png_data(f: Frame) -> bytes:
    stride = f.w * 3
    raw = b"".join(b"\0" + f.rgb[y * stride:(y + 1) * stride] for y in range(f.h))
    return zlib.compress(raw, 9)


def apng(frames: list[Frame]) -> bytes:
    """An animated PNG that loops `frames` forever: frame 0 whole, each later one a rectangle
    over the one before (dispose none, blend source). A viewer without APNG shows frame 0."""
    if not frames or (frames[0].x, frames[0].y, frames[0].w, frames[0].h) != (0, 0, W, H):
        raise ValueError("frame 0 must be the whole screen")
    out = [b"\x89PNG\r\n\x1a\n", _chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0)),
           _chunk(b"acTL", struct.pack(">II", len(frames), 0))]
    seq = 0
    for i, f in enumerate(frames):
        out.append(_chunk(b"fcTL", struct.pack(">IIIIIHHBB", seq, f.w, f.h, f.x, f.y,
                                               max(1, min(f.delay_ms, 65535)), 1000, 0, 0)))
        seq += 1
        data = _png_data(f)
        if i == 0:
            out.append(_chunk(b"IDAT", data))
        else:
            out.append(_chunk(b"fdAT", struct.pack(">I", seq) + data))
            seq += 1
    out.append(_chunk(b"IEND", b""))
    return b"".join(out)


def apng_data_url(frames: list[Frame]) -> str:
    return "data:image/png;base64," + b64encode(apng(frames)).decode()


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
    """The stored payload with its wall-clock time moved to the start of `now`'s minute (the
    title clock ticks on, and a clip, which runs the clock on for under a minute, never
    shows the minute change): `t` is local wall-clock seconds, as usage.add_clock_fields
    makes it."""
    if not payload:
        return None
    out = dict(payload)
    if "t" in out:
        t = int(now) + time.localtime(now).tm_gmtoff
        out["t"] = t - t % 60
    return out


# ---- the clock's own animation pack, for the page's approximate preview -------------------
# Without the renderer (public build) the page still draws the real Clawd: its sprites come
# from /clawd.pak on the user's own clock (GET /api/fs/get), kept in the user's config folder.
# The app itself never carries them. Format (clawd-home firmware/tools/clawd_assets.js):
# "CLWD", u16 version 1, u16 count, then a 72-byte descriptor per animation at offset 12.
PAK_MAX = 512_000
PAK_HEADER, PAK_DESC = 12, 72


def pak_count(data: bytes) -> int:
    """How many animations a clawd.pak holds; ValueError when it is not one (or a damaged one)."""
    if len(data) < PAK_HEADER or len(data) > PAK_MAX or data[:4] != b"CLWD":
        raise ValueError("not a clawd.pak")
    version, count = struct.unpack_from("<HH", data, 4)
    if version != 1 or not 0 < count < 64 or PAK_HEADER + count * PAK_DESC > len(data):
        raise ValueError("unknown clawd.pak version or count")
    for i in range(count):
        d = PAK_HEADER + i * PAK_DESC
        w, h = data[d + 16], data[d + 17]
        frames, _ls, _le, fbytes = struct.unpack_from("<4H", data, d + 20)
        frames_off, holds_off = struct.unpack_from("<II", data, d + 60)
        if (not w or not h or not frames or fbytes < (w * h + 1) // 2
                or frames_off + frames * fbytes > len(data) or holds_off + 2 * frames > len(data)):
            raise ValueError(f"clawd.pak animation {i} is damaged")
    return count


class PakCache:
    """The last clawd.pak read from a clock, on disk; `count` = what /api/heap said it held."""

    def __init__(self, path: Path) -> None:
        self._path = path
        self._meta = path.with_name(path.name + ".json")

    def load(self) -> tuple[bytes, int] | None:
        try:
            data = self._path.read_bytes()
            count = int(json.loads(self._meta.read_text())["count"])
            pak_count(data)
        except (OSError, ValueError, KeyError, TypeError):
            return None
        return data, count

    def save(self, data: bytes, count: int) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self._path.with_name(self._path.name + ".tmp")
        tmp.write_bytes(data)
        os.replace(tmp, self._path)
        self._meta.write_text(json.dumps({"count": count}))


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


def frame_key(layout: int, payload: dict | None, minute: int, kind: str = "still") -> str:
    # the title clock moves once a minute, so a frame is good for that long
    return f"{kind}|{layout}|{minute}|{json.dumps(payload, sort_keys=True)}"
