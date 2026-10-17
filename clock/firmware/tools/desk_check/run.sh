#!/bin/sh
# Decode a desk.pak with the clock's DeskPack.cpp and check every frame against
# the encoder's CRCs.   tools/desk_check/run.sh [desk.pak] [desk.crc.json]
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/.pio/desk_check"
mkdir -p "$out"
src="$root/src/features/clawd"
c++ -std=c++17 -O2 -Wall -Wextra -I"$src" "$root/tools/desk_check/main.cpp" "$src/DeskPack.cpp" -o "$out/desk_check"
"$out/desk_check" "${1:-$root/../build/desk/desk.pak}" "${2:-$root/../build/desk/desk.crc.json}"
