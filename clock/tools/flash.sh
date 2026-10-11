#!/bin/sh
# flash.sh — build the clock firmware (env smalltv_clawd) and flash it over WiFi.
#
#   tools/flash.sh              build, check the size, upload to /update, wait for the reboot
#   tools/flash.sh --no-build   upload the last build as is
#
# Clock address: $CLAWD_CLOCK, else the first http_targets entry in
# ~/.config/claude-usage-monitor/config, else 192.168.1.48.
# Web password (only if enabled on the clock): CLAWD_AUTH=user:pass (digest auth).
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
FW="$ROOT/smalltv/.pio/build/smalltv_clawd/firmware.bin"
MAX=524288   # clawd -> clawd OTA budget (smalltv/CLAWDMETER.md)

clock=${CLAWD_CLOCK:-}
if [ -z "$clock" ] && [ -f "$HOME/.config/claude-usage-monitor/config" ]; then
  clock=$(sed -n 's/^[[:space:]]*http_targets[[:space:]]*=[[:space:]]*\([^,#[:space:]]*\).*/\1/p' \
    "$HOME/.config/claude-usage-monitor/config" | head -n 1)
fi
clock=${clock:-192.168.1.48}
case $clock in http://*|https://*) base=$clock ;; *) base="http://$clock" ;; esac

build=1
for arg in "$@"; do
  case $arg in
    --no-build) build=0 ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

if [ $build -eq 1 ]; then
  echo "==> build (pio run -e smalltv_clawd)"
  (cd "$ROOT/smalltv" && pio run -e smalltv_clawd)
fi
[ -f "$FW" ] || { echo "no $FW, build first" >&2; exit 1; }

size=$(wc -c < "$FW" | tr -d ' ')
echo "==> firmware.bin $size B (max $MAX B)"
if [ "$size" -gt "$MAX" ]; then
  echo "too big: the clock cannot replace itself over OTA with this image" >&2
  exit 1
fi

set -- -sS --max-time 120
[ -n "${CLAWD_AUTH:-}" ] && set -- "$@" --digest -u "$CLAWD_AUTH"

uptime_of() { printf '%s' "$1" | sed -n 's/.*"uptime":\([0-9]*\).*/\1/p'; }
# For a while after a boot the clock takes ~10 s to answer any request: poll with 20 s.
before=$(uptime_of "$(curl -sS --max-time 20 "$base/api/heap" 2>/dev/null || true)")

echo "==> upload to $base/update"
resp=$(curl "$@" -w '\n%{http_code}' -F "firmware=@$FW" "$base/update") || {
  echo "upload failed (clock offline?)" >&2; exit 1; }
code=$(printf '%s\n' "$resp" | tail -n 1)
body=$(printf '%s\n' "$resp" | sed '$d')
if [ "$code" != 200 ]; then
  echo "clock answered $code: $body" >&2
  [ "$code" = 401 ] && echo "set CLAWD_AUTH=user:pass" >&2
  exit 1
fi
echo "    clock: $body, rebooting"

# Rebooted = uptime below the pre-upload uptime, or below 300 s (the reboot
# fires 1.2 s after the 200, before the first poll).
echo "==> waiting for the clock"
limit=300
[ -n "$before" ] && [ "$before" -gt $limit ] && limit=$before
sleep 5
i=0
while [ $i -lt 12 ]; do
  heap=$(curl -sS --max-time 20 "$base/api/heap" 2>/dev/null || true)
  up=$(uptime_of "$heap")
  if [ -n "$up" ] && [ "$up" -lt "$limit" ]; then
    ver=$(printf '%s' "$heap" | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')
    pak=$(printf '%s' "$heap" | sed -n 's/.*"pak":\([0-9]*\).*/\1/p')
    desk=$(printf '%s' "$heap" | sed -n 's/.*"desk":\([0-9]*\).*/\1/p')
    echo "    back up: version $ver, uptime ${up}s, pak $pak, desk $desk"
    [ "$pak" = 0 ] && echo "    warning: /clawd.pak missing" >&2
    [ "$desk" = 0 ] && echo "    warning: /desk.pak missing" >&2
    exit 0
  fi
  i=$((i + 1))
  sleep 2
done
echo "clock did not answer within ~4 min (12 polls); check it at $base" >&2
exit 1
