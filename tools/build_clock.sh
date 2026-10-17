#!/bin/sh
# Build what the app ships for the clock, from clock/firmware at this commit:
#   build/firmware/  the image, clawd.pak, desk.pak, style packs (tools/bundle_firmware.py)
#   build/renderer/  the clock-screen renderer for this computer (tools/build_preview.py)
# packaging/build_mac.sh and packaging/build_windows.ps1 then put both inside the app.
#
#   sh tools/build_clock.sh                firmware + renderer
#   sh tools/build_clock.sh --firmware     only the firmware bundle (what CI's firmware job does)
#   sh tools/build_clock.sh --renderer     only the renderer (the firmware bundle comes from CI)
#
# Needs PlatformIO (`pio`) for the firmware and a C++17 compiler for the renderer (CXX,
# CXXFLAGS; CLAWD_SIGN_IDENTITY signs the Mac renderer, else ad hoc). PYTHON picks the
# interpreter (default python3).
set -eu
cd "$(dirname "$0")/.."
PY=${PYTHON:-python3}
fw=1 renderer=1
case "${1:-}" in
  --firmware) renderer=0 ;;
  --renderer) fw=0 ;;
  "") ;;
  *) echo "build_clock.sh: unknown option $1" >&2; exit 2 ;;
esac

if [ $fw = 1 ]; then
  command -v pio >/dev/null 2>&1 || { echo "build_clock.sh: PlatformIO (pio) is not installed" >&2; exit 1; }
  (cd clock/firmware && pio run -e clawd -s)
  "$PY" tools/bundle_firmware.py
fi
if [ $renderer = 1 ]; then
  "$PY" tools/build_preview.py
fi
