#!/usr/bin/env python3
"""Build the clock-screen renderer the settings window previews with
(clawd_desktop/renderer/, git-ignored: it is compiled from the private firmware).

  python tools/build_preview.py --src ../clawd-home/firmware
  CLAWD_PRIVATE_FIRMWARE=1 sh packaging/build_mac.sh

Compiles tools/preview/clawd_render.cpp with the firmware's own drawing code
(<src>/src/features/clawd) and copies clawd.pak and the style packs (<src>/styles)
next to it, plus a preview.json that records the firmware commit and layouts. The window only trusts the renderer
when that matches the bundled firmware image, so it never shows a screen the
installed firmware cannot draw. Needs a gcc/clang-style C++17 compiler (set CXX
to pick one; on Windows use MinGW-w64 or clang).
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

# tools/ is on sys.path when this runs as a script
from bundle_firmware import copy_styles, from_source, style_api_from_source

ROOT = Path(__file__).resolve().parent.parent
DEST = ROOT / "clawd_desktop" / "renderer"
MAIN = ROOT / "tools" / "preview" / "clawd_render.cpp"
SOURCES = ("ClawdDraw.cpp", "ClawdAnim.cpp", "ClawdRate.cpp", "ClawdUi.cpp", "ClawdPack.cpp", "DeskPack.cpp",
           "StylePack.cpp")


def git_commit(src: Path) -> str:
    try:
        r = subprocess.run(["git", "-C", str(src), "rev-parse", "--short", "HEAD"],
                           capture_output=True, text=True, timeout=10, check=False)
    except (OSError, subprocess.SubprocessError):
        return ""
    return r.stdout.strip() if r.returncode == 0 else ""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", type=Path, help="the firmware dir (holds src/ and data/)")
    ap.add_argument("--commit", default="", help="firmware commit (default: git HEAD of --src)")
    ap.add_argument("--clear", action="store_true", help="remove the built renderer instead")
    a = ap.parse_args()
    if a.clear:
        try:
            if DEST.exists():
                shutil.rmtree(DEST)
        except OSError as e:
            print(f"could not remove {DEST}: {e}")
            return 1
        print("preview renderer removed")
        return 0
    if not a.src:
        print("give --src")
        return 1
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

    DEST.mkdir(parents=True, exist_ok=True)
    exe = DEST / ("clawd_render.exe" if sys.platform == "win32" else "clawd_render")
    cxx = os.environ.get("CXX", "c++")
    cmd = [cxx, "-std=c++17", "-O2", "-Wall", "-Wextra", "-Wno-unused-function", "-Wno-missing-field-initializers",
           f"-I{clawd}", str(MAIN), *[str(clawd / n) for n in SOURCES], "-o", str(exe)]
    if sys.platform == "win32":
        cmd += ["-static"]
    print(" ".join(cmd))
    r = subprocess.run(cmd, check=False)
    if r.returncode != 0 or not exe.is_file():
        print("compile failed")
        return 1
    shutil.copyfile(pak, DEST / "clawd.pak")
    try:
        styles = copy_styles(a.src, style_api, DEST / "styles")
    except (OSError, ValueError, KeyError, TypeError) as e:
        print(f"styles: {e}")
        return 1
    (DEST / "preview.json").write_text(json.dumps({
        "exe": exe.name, "pak": "clawd.pak", "commit": commit, "fwSeq": seq, "layouts": layouts,
        "styleApi": style_api,
    }, indent=2) + "\n")
    print(f"built {exe} (firmware {commit or '?'}, fwSeq {seq}, layouts {layouts}, {styles} style packs)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
