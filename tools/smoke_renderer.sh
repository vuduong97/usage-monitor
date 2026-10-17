#!/bin/sh
# Smoke test of a built clock-screen renderer (tools/build_preview.py): the waiting screen,
# every style pack it carries, and one animated clip. Each frame is 240x240 RGB565.
#
#   sh tools/smoke_renderer.sh [build/renderer]
#
# PYTHON picks the interpreter (default python3).
set -eu
d=${1:-build/renderer}
PY=${PYTHON:-python3}
exe=$("$PY" -c "import json,sys;print(json.load(open(sys.argv[1]))['exe'])" "$d/preview.json")
run="$d/$exe"
payload='{"s":42,"sr":134,"w":67,"wr":3960,"t":1760191500,"tf":24}'
tmp=${RUNNER_TEMP:-${TMPDIR:-/tmp}}/clawd-smoke.$$
trap 'rm -f "$tmp"' EXIT

n=$(: | "$run" --pak "$d/clawd.pak" | wc -c | tr -d ' ')
[ "$n" = 172800 ] || { echo "smoke: classic screen gave $n bytes" >&2; exit 1; }
count=0
for p in "$d"/styles/*.pak; do
  [ -e "$p" ] || continue
  n=$(echo "$payload" | "$run" --pak "$d/clawd.pak" --style "$p" | wc -c | tr -d ' ')
  [ "$n" = 172800 ] || { echo "smoke: $p gave $n bytes" >&2; exit 1; }
  count=$((count + 1))
done
# to a file, not `| head`: head closes the pipe early
echo "$payload" | "$run" --pak "$d/clawd.pak" --clip > "$tmp"
[ "$(head -c 4 "$tmp")" = CLP1 ] || { echo "smoke: --clip gave no CLP1" >&2; exit 1; }
echo "renderer ok: classic screen, $count style packs, an animated clip"
