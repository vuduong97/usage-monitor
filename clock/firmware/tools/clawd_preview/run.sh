#!/bin/sh
# Build the desktop preview of the Clawd screens and write PNG screenshots.
#   tools/clawd_preview/run.sh [out_dir]      (needs a C++17 compiler + ImageMagick)
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-"$root/.pio/clawd_preview"}
mkdir -p "$out"
src="$root/src/features/clawd"
c++ -std=c++17 -O1 -Wall -Wextra -Wno-unused-function -Wno-missing-field-initializers \
  -I"$src" "$root/tools/clawd_preview/main.cpp" "$src/ClawdDraw.cpp" "$src/ClawdAnim.cpp" \
  "$src/ClawdRate.cpp" "$src/ClawdUi.cpp" "$src/ClawdPack.cpp" "$src/DeskPack.cpp" "$src/StylePack.cpp" -o "$out/clawd_preview"
DESK_PACK_PATH="${DESK_PACK_PATH:-$root/../build/desk/desk.pak}" CLAWD_PACK_PATH="$root/data/clawd.pak" CLAWD_STYLE_DIR="${CLAWD_STYLE_DIR:-$root/styles}" "$out/clawd_preview" "$out" | while read -r line; do
  case "$line" in
    *.ppm) magick "$line" -scale 200% "${line%.ppm}.png" && rm "$line" && echo "${line%.ppm}.png" ;;
    *) echo "$line" ;;
  esac
done
