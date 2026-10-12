#!/bin/sh
# Host test for Settings.cpp (no hardware): applies a config that still carries
# keys of features this firmware no longer has, saves and reloads it through the
# fake flash, and checks the result against fixtures/expected.json.
#   fixtures/full-legacy.json  a populated config from the old multi-feature build
#   fixtures/expected.json     what this firmware must keep from it (live keys only)
# Needs: a C++17 compiler, python3, and ArduinoJson (pio run once to fetch it, or
# ARDUINOJSON_DIR=<path to ArduinoJson/src>).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin=$(mktemp)
trap 'rm -f "$bin"' EXIT
"$here/build.sh" "$here/../../src" "$bin" -DARDUINO=100 -DARDUINOJSON_ENABLE_PROGMEM=0
"$bin" < "$here/fixtures/full-legacy.json" > "$bin.out"
python3 - "$bin.out" "$here/fixtures/expected.json" <<'PY'
import json, sys
got, want = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
if got != want:
    for k in sorted(set(got) | set(want)):
        if got.get(k) != want.get(k):
            print(f"DIFF {k}: got {got.get(k)!r} want {want.get(k)!r}")
    sys.exit("settings round-trip FAILED")
print("settings round-trip ok:", len(want), "top-level keys kept, dead keys dropped")
PY
rm -f "$bin.out"
