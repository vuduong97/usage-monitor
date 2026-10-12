# pyright: reportUndefinedVariable=false
# ruff: noqa: F821
# (SCons script: PlatformIO injects Import and env)
"""clawd: fail the build if Arduino_GFX's YCbCr tables are linked again.

gfx_trim.py (pre) compiles writeYCbCrPixels out so these ~6.8 KB of DRAM
tables drop out of the image. A post script, because PROGNAME is only set
after the pre scripts run.
"""
import os
import re
import subprocess

Import("env")

TABLES = re.compile(r"\b(CLIP[RGB]?(BE)?|Y2I16|CR2R16|CR2G16|CB2G16|CB2B16)$")


def _check_dram(source, target, env):
    elf = str(target[0])
    nm = os.path.join(os.path.dirname(env.subst("$CC")), "xtensa-lx106-elf-nm")
    if not os.path.isfile(nm):
        nm = "xtensa-lx106-elf-nm"
    try:
        out = subprocess.run([nm, "-C", elf], capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError) as e:
        raise SystemExit(f"gfx_check.py: {nm} failed: {e}") from e
    back = sorted({line.split()[-1] for line in out.splitlines() if TABLES.search(line)})
    if back:
        raise SystemExit("gfx_check.py: Arduino_GFX YCbCr tables are linked again "
                         "(CLAWD_NO_YCBCR / gfx_trim.py): " + ", ".join(back))
    print("gfx_check.py: no YCbCr tables in the image")


env.AddPostAction("$BUILD_DIR/${PROGNAME}.elf", _check_dram)
