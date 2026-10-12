"""The app icon, drawn in code: an orange rounded square with a white clock face,
plus a status dot (green connected, amber searching, red needs attention).
No third-party art is involved."""
from __future__ import annotations

import math

from PIL import Image, ImageDraw

ORANGE = (222, 117, 82, 255)
WHITE = (255, 255, 255, 255)
DOT = {
    "ok": (52, 199, 89, 255),
    "busy": (255, 179, 0, 255),
    "error": (230, 57, 70, 255),
}


def base_icon(size: int = 256) -> Image.Image:
    s = 4  # draw big, then downsample for smooth edges
    big = size * s
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    pad = big * 0.06
    d.rounded_rectangle((pad, pad, big - pad, big - pad), radius=big * 0.22, fill=ORANGE)
    c = big / 2
    r = big * 0.27
    w = max(2, int(big * 0.055))
    d.ellipse((c - r, c - r, c + r, c + r), outline=WHITE, width=w)
    for ang, length in ((-90, 0.72), (30, 0.52)):            # 12 and ~4 o'clock hands
        a = math.radians(ang)
        d.line((c, c, c + math.cos(a) * r * length, c + math.sin(a) * r * length),
               fill=WHITE, width=w)
    d.ellipse((c - w, c - w, c + w, c + w), fill=WHITE)
    return img.resize((size, size), Image.LANCZOS)


def status_icon(state: str, size: int = 64) -> Image.Image:
    img = base_icon(size)
    color = DOT.get(state)
    if color is None:
        return img
    d = ImageDraw.Draw(img)
    r = size * 0.19
    cx = cy = size - r - size * 0.02
    d.ellipse((cx - r - 2, cy - r - 2, cx + r + 2, cy + r + 2), fill=(255, 255, 255, 255))
    d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=color)
    return img
