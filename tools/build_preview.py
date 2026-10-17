#!/usr/bin/env python3
"""Build the clock-screen renderer the settings window previews with: build/renderer/
(git-ignored), compiled from clock/firmware's own drawing code. packaging/build_mac.sh and
build_windows.ps1 ship it inside the app next to the firmware (tools/bundle_firmware.py).

  python3 tools/build_preview.py                   # clock/firmware -> build/renderer

On macOS the program is stripped and signed (CLAWD_SIGN_IDENTITY, else ad hoc); on
Windows it is linked static and stripped. CXXFLAGS adds compiler flags (a universal
Mac file: "-arch arm64 -arch x86_64").

Compiles tools/preview/clawd_render.cpp with the firmware's own drawing code
(<src>/src/features/clawd) and copies clawd.pak and the style packs (<src>/styles)
next to it, plus a preview.json that records the firmware commit and layouts. The window only
trusts the renderer when that matches the bundled firmware image, so it never shows a screen
the installed firmware cannot draw. Needs a gcc/clang-style C++17 compiler (set CXX to pick
one; on Windows use MinGW-w64 or clang).
"""
from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
from pathlib import Path

# tools/ is on sys.path when this runs as a script
from bundle_firmware import SRC, copy_styles, from_source, git_commit, style_api_from_source

ROOT = Path(__file__).resolve().parent.parent
DEST = ROOT / "build" / "renderer"
MAIN = ROOT / "tools" / "preview" / "clawd_render.cpp"
# The clawd_render interface this tools/preview/clawd_render.cpp speaks (arguments, stdin,
# stdout, CLP1): RENDER_API in crates/clawd-firmware/src/preview.rs. Bump both when it changes, so an app
# never runs a renderer it cannot read.
RENDER_API = 1
SOURCES = ("ClawdDraw.cpp", "ClawdAnim.cpp", "ClawdRate.cpp", "ClawdUi.cpp", "ClawdPack.cpp", "DeskPack.cpp",
           "StylePack.cpp")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", type=Path, default=SRC, help=f"the firmware dir (holds src/ and data/; default {SRC})")
    ap.add_argument("--commit", default="", help="firmware commit (default: git HEAD of --src)")
    ap.add_argument("--clear", action="store_true", help="remove the built renderer instead")
    ap.add_argument("--out", type=Path, default=DEST, help=f"build into this folder (default {DEST})")
    ap.add_argument("--windows", action="store_true", default=sys.platform == "win32",
                    help="build a Windows .exe (default on Windows; with a MinGW CXX elsewhere)")
    a = ap.parse_args()
    dest: Path = a.out
    if a.clear:
        try:
            if dest.exists():
                shutil.rmtree(dest)
        except OSError as e:
            print(f"could not remove {dest}: {e}")
            return 1
        print("preview renderer removed")
        return 0
    clawd = a.src / "src" / "features" / "clawd"
    pak = a.src / "data" / "clawd.pak"
    missing = [str(p) for p in [clawd / n for n in SOURCES] + [pak] if not p.is_file()]
    if missing:
        print("not found: " + ", ".join(missing))
        return 1
    try:
        seq, layouts = from_source(a.src)
        style_api = style_api_from_source(a.src)
    except (OSError, ValueError) as e:
        print(f"--src: {e}")
        return 1
    commit = a.commit or git_commit(a.src)

    dest.mkdir(parents=True, exist_ok=True)
    exe = dest / ("clawd_render.exe" if a.windows else "clawd_render")
    cxx = os.environ.get("CXX", "c++")
    cmd = [cxx, "-std=c++17", "-O2", "-Wall", "-Wextra", "-Wno-unused-function", "-Wno-missing-field-initializers",
           *shlex.split(os.environ.get("CXXFLAGS", "")),     # e.g. "-arch arm64 -arch x86_64" (one Mac file)
           f"-I{clawd}", str(MAIN), *[str(clawd / n) for n in SOURCES], "-o", str(exe)]
    ui_h = (clawd / "ClawdUi.h").read_text(encoding="utf-8")
    if "source[" in ui_h:
        cmd.insert(1, "-DCLAWD_HAS_SOURCE")      # firmware that labels whose usage it shows (src)
    if "why[" in ui_h:
        cmd.insert(1, "-DCLAWD_HAS_WHY")         # firmware that says why there is no usage (login)
    if a.windows:
        cmd += ["-static", "-s"]                  # no MinGW DLLs to ship; no symbol names in the file
        cmd.insert(1, "-D_POSIX_THREAD_SAFE_FUNCTIONS")   # MinGW's time.h declares gmtime_r only then
    print(" ".join(cmd))
    r = subprocess.run(cmd, check=False)
    if r.returncode != 0 or not exe.is_file():
        print("compile failed")
        return 1
    shutil.copyfile(pak, dest / "clawd.pak")
    try:
        styles = copy_styles(a.src, style_api, dest / "styles")
    except (OSError, ValueError, KeyError, TypeError) as e:
        print(f"styles: {e}")
        return 1
    (dest / "preview.json").write_text(json.dumps({
        "exe": exe.name, "pak": "clawd.pak", "commit": commit, "fwSeq": seq, "layouts": layouts,
        "styleApi": style_api, "renderApi": RENDER_API, "desktop": git_commit(ROOT),
    }, indent=2) + "\n")
    print(f"built {exe} (firmware {commit or '?'}, fwSeq {seq}, layouts {layouts}, {styles} style packs)")
    if sys.platform == "darwin" and not a.windows and not finish_mac(exe):
        return 1
    return 0


def finish_mac(exe: Path) -> bool:
    """Strip the symbol names, then sign: with CLAWD_SIGN_IDENTITY ("Developer ID Application:
    ...", what the app is signed with), else ad hoc; Apple silicon runs no unsigned program."""
    ident = os.environ.get("CLAWD_SIGN_IDENTITY") or "-"
    for cmd in (["strip", "-x", str(exe)],
                ["codesign", "--force", "--timestamp" if ident != "-" else "--timestamp=none",
                 "--options", "runtime", "-s", ident, str(exe)]):
        if subprocess.run(cmd, check=False).returncode != 0:
            print("failed: " + " ".join(cmd))
            return False
    return True


if __name__ == "__main__":
    raise SystemExit(main())
