#!/bin/sh
# provision.sh: turn a stock JUZIPi SmallTV SD Pro into a clawd clock ready to gift.
#
#   tools/provision.sh <clock-ip> [--dry-run] [--yes]
#
# Before: join the stock clock to this bench WiFi by hand (its own setup page),
# and note the IP it shows. Then this script, over WiFi only:
#   1. checks the clock runs the stock firmware with the known OTA page (SDPro 1.0.x)
#   2. uploads clawd.pak + a config.json (bench WiFi) through the stock /upload
#   3. flashes firmware.bin through the stock /update_ota, waits for clawd to boot
#   4. backs up the stock files (tools/stock_fs.sh backup, verified) and deletes them
#   5. uploads desk.pak (~75 s)
#   6. sets hostname clawd-<chip id>, setup WiFi Clawd-<CHIP ID> with a random
#      password, a random web login (admin), and forgets the bench WiFi: the
#      clock reboots into SETUP MODE, ready for its new owner
#   7. prints the card to put in the box (also appended to .stock-backup/provision/cards.tsv)
#
# Every step checks first and skips what is already done, so a clock that
# stopped half way can be run again (on clawd it starts at step 4).
# --dry-run: read-only probes run, every change is printed instead of sent.
#
# Needs: build/desk/desk.pak, firmware/data/clawd.pak, firmware/.pio/build/clawd/firmware.bin
# (`pio run -e clawd`), and the bench WiFi in CLAWD_BENCH_SSID / CLAWD_BENCH_PASS
# (asked for when missing). Passwords and backups land in .stock-backup/ (gitignored).
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
FW=${CLAWD_FW:-"$ROOT/firmware/.pio/build/clawd/firmware.bin"}
CLAWD_PAK=${CLAWD_PAK:-"$ROOT/firmware/data/clawd.pak"}
DESK_PAK=${CLAWD_DESK_PAK:-"$ROOT/build/desk/desk.pak"}
OUT="$ROOT/.stock-backup/provision"
FW_MAX=532480       # 520 KB: keeps clawd -> clawd OTA possible (stock -> clawd allows 552,960)
OURS='^/(config\.json|clawd\.pak|desk\.pak|.*\.tmp)$'

ip="" dry=0 yes=0
for arg in "$@"; do
  case $arg in
    --dry-run) dry=1 ;;
    --yes) yes=1 ;;
    -h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) ip=$arg ;;
  esac
done
[ -n "$ip" ] || { sed -n '2,4p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
H="http://${ip#http://}"
H=${H%/}

say() { printf '==> %s\n' "$*"; }
die() { printf 'provision: %s\n' "$*" >&2; exit 1; }
# Mutating calls go through act: printed in --dry-run, run otherwise.
act() {
  if [ $dry -eq 1 ]; then printf '    [dry-run] %s\n' "$*" >&2; return 0; fi
  "$@"
}
# The ESP8266 answers slowly for minutes after a boot: 20 s per read.
get() { curl -fsS --max-time 20 "$@"; }
json_str() { sed -n "s/.*\"$1\":\"\\([^\"]*\\)\".*/\\1/p"; }
json_num() { sed -n "s/.*\"$1\":\\([0-9][0-9]*\\).*/\\1/p"; }
randpw() { LC_ALL=C tr -dc 'abcdefghjkmnpqrstuvwxyz23456789' < /dev/urandom | head -c "$1"; }

for f in "$FW" "$CLAWD_PAK" "$DESK_PAK" "$ROOT/tools/stock_fs.sh"; do
  [ -f "$f" ] || die "missing $f"
done
fw_size=$(wc -c < "$FW" | tr -d ' ')
[ "$fw_size" -le $FW_MAX ] || die "firmware.bin is $fw_size B, over $FW_MAX B"
mkdir -p "$OUT"
chmod 700 "$ROOT/.stock-backup" "$OUT" 2>/dev/null || true

# ---- what is on the clock? -------------------------------------------------------
heap=$(get "$H/api/heap" 2>/dev/null || true)
if printf '%s' "$heap" | grep -q '"pak"'; then
  state=clawd
elif get "$H/config" 2>/dev/null | grep -q '"freespace"'; then
  state=stock
else
  die "$H answers neither as stock nor as clawd firmware (wrong IP, or still booting?)"
fi
say "clock at $H runs $state firmware"

# ---- 1-3: stock -> clawd -------------------------------------------------------
if [ $state = stock ]; then
  js=$(get "$H/javascript.js") || die "could not read the stock web page"
  for want in '"/update_ota"' 'append("update"' 'startsWith("SDP")'; do
    printf '%s' "$js" | grep -qF "$want" ||
      die "this stock firmware's OTA page differs from SDPro 1.0.6 (no $want): stop and check by hand"
  done

  ssid=${CLAWD_BENCH_SSID:-}
  pass=${CLAWD_BENCH_PASS:-}
  if [ -z "$ssid" ]; then printf 'Bench WiFi name (the one this Mac and the clock are on): '; read -r ssid; fi
  if [ -z "$pass" ]; then
    printf 'Bench WiFi password: '; stty -echo 2>/dev/null || true; read -r pass; stty echo 2>/dev/null || true; echo
  fi
  [ -n "$ssid" ] || die "no bench WiFi name"
  cfg=$(mktemp)
  trap 'rm -f "$cfg"' EXIT
  python3 -c 'import json,sys; print(json.dumps({"wifi":[{"ssid":sys.argv[1],"pass":sys.argv[2]}],"mode":"usage"}))' \
    "$ssid" "$pass" > "$cfg"

  if [ $yes -eq 0 ] && [ $dry -eq 0 ]; then
    printf 'Flash clawd onto the stock clock at %s? [y/N] ' "$ip"; read -r ok
    [ "$ok" = y ] || [ "$ok" = Y ] || die "stopped"
  fi
  say "upload clawd.pak + config.json (stock /upload)"
  act curl -fsS --max-time 120 -F "file=@$CLAWD_PAK;filename=clawd.pak" "$H/upload" >/dev/null
  act curl -fsS --max-time 60 -F "file=@$cfg;filename=config.json" "$H/upload" >/dev/null
  say "flash firmware.bin, $fw_size B (stock /update_ota)"
  act curl -fsS --max-time 300 -F "update=@$FW;filename=SDP_clawd.bin" "$H/update_ota" >/dev/null
  [ $dry -eq 1 ] && { say "dry run: stopping here (the clock would now reboot into clawd)"; exit 0; }

  say "waiting for clawd to boot"
  sleep 15
  i=0 heap=
  while [ $i -lt 30 ]; do
    heap=$(get "$H/api/heap" 2>/dev/null || true)
    printf '%s' "$heap" | grep -q '"pak"' && break
    i=$((i + 1)); sleep 4
  done
  printf '%s' "$heap" | grep -q '"pak"' ||
    die "clawd did not answer at $H within ~3 min. If the bench router gave it a new IP, rerun with that IP"
  state=clawd
fi

ver=$(printf '%s' "$heap" | json_str version)
pak=$(printf '%s' "$heap" | json_num pak)
say "clawd $ver, clawd.pak screens: ${pak:-?}"
[ "${pak:-0}" -gt 0 ] || die "/clawd.pak missing or invalid on the clock"

cfgnow=$(get "$H/api/config") || die "cannot read /api/config (web login already set? use CLAWD_AUTH=user:pass)"
host=$(printf '%s' "$cfgnow" | json_str hostname)
case $host in
  smalltv-*) id=${host#smalltv-} ;;
  clawd-*) id=${host#clawd-} ;;
  *) id=$(randpw 4) ;;          # custom name: no chip id to reuse
esac
newhost="clawd-$id"
ap="Clawd-$(printf '%s' "$id" | tr '[:lower:]' '[:upper:]')"
dir="$OUT/$newhost-$(date +%Y%m%d-%H%M%S)"

# ---- 4: stock files off ----------------------------------------------------------
export CLAWD_CLOCK="$H"
stock=$(get "$H/api/fs/list" | cut -f2 | grep -Ev "$OURS" || true)
if [ -n "$stock" ]; then
  say "$(printf '%s\n' "$stock" | wc -l | tr -d ' ') stock files on the clock: back up, verify, delete"
  if [ $dry -eq 1 ]; then
    printf '    [dry-run] tools/stock_fs.sh backup %s && tools/stock_fs.sh delete %s\n' "$dir" "$dir"
  else
    sh "$ROOT/tools/stock_fs.sh" backup "$dir"
    sh "$ROOT/tools/stock_fs.sh" delete "$dir"
  fi
else
  say "no stock files left"
fi

# ---- 5: desk.pak ---------------------------------------------------------------
desk=$(get "$H/api/heap" | json_num desk)
if [ "${desk:-0}" -gt 0 ]; then
  say "desk.pak already there ($desk animations)"
else
  say "upload desk.pak ($(wc -c < "$DESK_PAK" | tr -d ' ') B, ~75 s)"
  act curl -fsS --max-time 600 -F "file=@$DESK_PAK" "$H/api/deskpak" >/dev/null
  if [ $dry -eq 0 ]; then
    desk=$(get "$H/api/heap" | json_num desk)
    [ "${desk:-0}" -gt 0 ] || die "desk.pak did not load"
    say "desk.pak: $desk animations"
  fi
fi

# ---- 6: identity, logins, forget the bench WiFi --------------------------------
webpw=$(randpw 10)
appw=$(randpw 8)
body=$(python3 -c 'import json,sys
h,ap,appw,pw=sys.argv[1:]
print(json.dumps({"hostname":h,"apSsid":ap,"apPass":appw,"wifi":[],"mode":"usage",
                  "auth":{"enabled":True,"user":"admin","pass":pw}}))' "$newhost" "$ap" "$appw" "$webpw")
say "set hostname $newhost, setup WiFi $ap, web login admin, forget the bench WiFi"
if [ $dry -eq 1 ]; then
  printf '    [dry-run] POST %s/api/config {"hostname":"%s","apSsid":"%s","apPass":"***","wifi":[],"auth":{...}}\n' \
    "$H" "$newhost" "$ap"
  say "dry run done; nothing was changed"
  exit 0
fi
resp=$(curl -fsS --max-time 30 -H 'Content-Type: application/json' --data-binary "$body" "$H/api/config") ||
  die "saving the config failed"
printf '%s' "$resp" | grep -q '"reboot":true' || die "clock did not schedule a reboot: $resp"

# ---- 7: the card ------------------------------------------------------------------
[ -f "$OUT/cards.tsv" ] || printf 'date\thostname\tsetup_wifi\tsetup_pass\tweb_user\tweb_pass\tfirmware\n' > "$OUT/cards.tsv"
printf '%s\t%s\t%s\t%s\tadmin\t%s\t%s\n' "$(date +%F)" "$newhost" "$ap" "$appw" "$webpw" "$ver" >> "$OUT/cards.tsv"
chmod 600 "$OUT/cards.tsv"
cat <<EOF

  Done. The clock reboots into SETUP MODE showing $ap.
  ┌────────────────────────────────────────────────┐
  │  Clawd  ·  $newhost
  │  WiFi cài đặt:   $ap
  │  Mật khẩu WiFi:  $appw
  │  Trang cài đặt:  http://192.168.4.1
  │  Đăng nhập:      admin / $webpw
  └────────────────────────────────────────────────┘
  (saved in $OUT/cards.tsv)
EOF
