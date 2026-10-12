# pyright: reportUndefinedVariable=false
# ruff: noqa: F821
# (SCons script: PlatformIO injects Import and env)
"""clawd: drop Arduino_GFX's YCbCr -> RGB565 path (~6.8 KB).

Arduino_DataBus::writeYCbCrPixels is virtual, so every bus vtable links it,
and with it the lookup tables of YCbCr2RGB.h (CLIP*BE, Y2I16, CR2R16, ...).
Those are plain `static const` arrays, which the ESP8266 keeps in DRAM:
6,804 B of heap for a JPEG/MJPEG path the clock never takes. With
-DCLAWD_NO_YCBCR (platformio.ini) the function body is compiled out, the
tables lose their only user and the linker drops them (flash and RAM).

The library is pinned (platformio.ini); if a new version moves the function,
the patch fails loudly instead of silently doing nothing, and gfx_check.py
(post) fails the build if the tables are linked again.
"""
import os

Import("env")

MARK = "// clawd: CLAWD_NO_YCBCR"
SIG = "void Arduino_DataBus::writeYCbCrPixels("


def _patch(path):
    try:
        with open(path, encoding="utf-8") as f:
            src = f.read()
    except OSError as e:
        raise SystemExit(f"gfx_trim.py: cannot read {path}: {e}") from e
    if MARK in src:
        return
    start = src.find(SIG)
    if start < 0:
        raise SystemExit(f"gfx_trim.py: {SIG} not found in {path} (Arduino_GFX changed?)")
    open_brace = src.index("{", start)
    depth, i = 0, open_brace
    while i < len(src):
        c = src[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                break
        i += 1
    if depth:
        raise SystemExit(f"gfx_trim.py: unbalanced braces after {SIG} in {path}")
    body = src[open_brace + 1:i]
    new = ("{\n" + MARK + "\n#if defined(CLAWD_NO_YCBCR)\n"
           "  (void)yData; (void)cbData; (void)crData; (void)w; (void)h;\n"
           "#else" + body + "#endif\n")
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(src[:open_brace] + new + src[i:])
    except OSError as e:
        raise SystemExit(f"gfx_trim.py: cannot patch {path}: {e}") from e
    print("gfx_trim.py: compiled out writeYCbCrPixels in " + path)


def _patch_libdeps():
    libdeps = os.path.join(env.subst("$PROJECT_LIBDEPS_DIR"), env.subst("$PIOENV"))
    try:
        names = sorted(os.listdir(libdeps))
    except OSError as e:
        raise SystemExit(f"gfx_trim.py: no library dir {libdeps}: {e}") from e
    found = False
    for name in names:
        p = os.path.join(libdeps, name, "src", "Arduino_DataBus.cpp")
        if os.path.isfile(p):
            _patch(p)
            found = True
    if not found:
        raise SystemExit("gfx_trim.py: Arduino_DataBus.cpp not found under " + libdeps)


_patch_libdeps()
