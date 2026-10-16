#!/bin/sh
# Host test for Settings.cpp (no hardware): applies a config that still carries
# keys of features this firmware no longer has, saves and reloads it through the
# fake flash, and checks the result against fixtures/expected.json.
#   fixtures/full-legacy.json  a populated config from the old multi-feature build
#   fixtures/expected.json     what this firmware must keep from it (live keys only)
#   fixtures/ui-save.json      the web UI's Save body; expected-after-ui-save.json is the result
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
# What the web UI POSTs on Save (no passwords in it) applied over the stored
# config: every secret must survive.
"$bin" "$here/fixtures/ui-save.json" < "$here/fixtures/full-legacy.json" > "$bin.out"
python3 - "$bin.out" "$here/fixtures/expected-after-ui-save.json" <<'PY'
import json, sys
got, want = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
if got != want:
    sys.exit("UI-save secret preservation FAILED")
print("ui save keeps wifi/ap/auth secrets ok")
PY
rm -f "$bin.out"
