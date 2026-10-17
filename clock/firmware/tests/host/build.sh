#!/bin/sh
# Build settings_dump against a firmware src dir:  build.sh <src_dir> <out_binary> [extra c++ flags]
# The files Settings.cpp needs are staged next to the host stand-ins first,
# because a quoted #include looks in the including file's own directory before
# any -I path, and src/Platform.h would otherwise win over the stand-in.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$1; out=$2; shift 2
aj=${ARDUINOJSON_DIR:-$(cd "$here/../.." && pwd)/.pio/libdeps/clawd/ArduinoJson/src}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for f in Settings.cpp Settings.h config.h AtomicJson.h SettingsTransaction.h; do
  [ -f "$src/$f" ] && cp "$src/$f" "$tmp/"
done
cp "$src"/board_*.h "$tmp/"
cp "$here/Arduino.h" "$here/Platform.h" "$here/LittleFS.h" "$here/settings_dump.cpp" "$tmp/"
c++ -std=c++17 -Wno-unused-parameter -I"$tmp" -I"$aj" "$@" "$tmp/settings_dump.cpp" "$tmp/Settings.cpp" -o "$out"
