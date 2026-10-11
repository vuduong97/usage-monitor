#!/usr/bin/env python3
"""Regenerate src/webui.h from src/webui.html.

The web UI ships as a gzip-compressed byte array in PROGMEM and is served with
`Content-Encoding: gzip` (see handleRoot in src/WebPortal.cpp). src/webui.html
is the source of truth — edit it, then rerun:

    python tools/gzip_webui.py

Python 3 stdlib only. Output is deterministic (gzip header mtime=0), so
re-running without changes leaves webui.h untouched.
"""
import gzip
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "src" / "webui.html"
OUT = ROOT / "src" / "webui.h"
# The Clawdmeter build (SMALLTV_CLAWD) compiles these features out; the page
# would remove their tabs at runtime anyway (hideFeat), so drop the markup and
# save the flash -- that image has to fit the clock's stock OTA budget.
CLAWD_OUT = ROOT / "src" / "webui_clawd.h"
CLAWD_STRIP = ("ticker", "radar", "ha")

HEADER = """\
// GENERATED FILE -- do not edit by hand.
// Produced by tools/gzip_webui.py from src/webui.html (the source of truth).
// Regenerate after editing the HTML:  python tools/gzip_webui.py
//
// Gzip-compressed single-page config UI, served from PROGMEM with
// `Content-Encoding: gzip` by handleRoot in WebPortal.cpp.
#pragma once
#include <Arduino.h>

static const uint8_t WEBUI_HTML_GZ[] PROGMEM = {
"""


def write(raw: bytes, out: pathlib.Path) -> None:
    blob = gzip.compress(raw, compresslevel=9, mtime=0)
    lines = [HEADER]
    for i in range(0, len(blob), 16):
        chunk = blob[i:i + 16]
        lines.append("  " + ", ".join(f"0x{b:02x}" for b in chunk) + ",\n")
    lines.append("};\n")
    lines.append("static const size_t WEBUI_HTML_GZ_LEN = sizeof(WEBUI_HTML_GZ);\n")
    out.write_text("".join(lines), encoding="utf-8", newline="\n")
    print(f"{SRC.name}: {len(raw)} B -> gzip {len(blob)} B "
          f"({100 * len(blob) / len(raw):.1f}%) -> {out.name}")


def strip_sections(html: str, ids) -> str:
    for sid in ids:
        html, n = re.subn(r'\n?[ \t]*<!--[^\n]*-->\n[ \t]*<section id="%s" class="tab">.*?</section>' % sid,
                          "", html, count=1, flags=re.S)
        if n != 1:
            html, n = re.subn(r'<section id="%s" class="tab">.*?</section>' % sid, "", html,
                              count=1, flags=re.S)
        if n != 1:
            raise SystemExit(f"section {sid!r} not found")
    return html


def main() -> None:
    raw = SRC.read_bytes()
    write(raw, OUT)
    write(strip_sections(raw.decode("utf-8"), CLAWD_STRIP).encode("utf-8"), CLAWD_OUT)


if __name__ == "__main__":
    main()
