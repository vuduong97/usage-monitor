"""Writes icons/icon.png and icons/icon.ico: a plain orange disc, so the spike needs no artwork.
Run once: python make_icon.py (pure standard library)."""
import struct
import zlib
from pathlib import Path

SIZE = 64


def pixels() -> bytes:
    rows = []
    c = (SIZE - 1) / 2
    for y in range(SIZE):
        row = bytearray([0])  # PNG filter type 0
        for x in range(SIZE):
            inside = (x - c) ** 2 + (y - c) ** 2 <= (SIZE / 2 - 2) ** 2
            row += bytes((217, 119, 87, 255) if inside else (0, 0, 0, 0))
        rows.append(bytes(row))
    return b"".join(rows)


def png() -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(pixels(), 9)) + chunk(b"IEND", b"")


def ico(png_bytes: bytes) -> bytes:
    entry = struct.pack("<BBBBHHII", SIZE, SIZE, 0, 0, 1, 32, len(png_bytes), 22)
    return struct.pack("<HHH", 0, 1, 1) + entry + png_bytes


out = Path(__file__).parent / "icons"
out.mkdir(exist_ok=True)
data = png()
(out / "icon.png").write_bytes(data)
(out / "icon.ico").write_bytes(ico(data))
