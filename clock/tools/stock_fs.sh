#!/bin/sh
# stock_fs.sh — move the stock firmware's files off the clock and back.
#
# The clock's 3 MB LittleFS is shared with the stock JUZIPi firmware, whose web
# UI, fonts, icons, flip-clock digits and photos take ~1.5 MB. clawd-home needs
# that space for desk.pak, so the stock files live on this Mac instead. They
# MUST go back on the clock before flashing the stock firmware again
# (`restore`), or the stock web UI and clock faces are gone.
#
#   tools/stock_fs.sh backup  [dir]   list + download every file and a raw image of
#                                     the whole partition (default .stock-backup/fs-<date>)
#   tools/stock_fs.sh verify  dir     unpack the raw image and compare it with the files
#   tools/stock_fs.sh delete  dir     delete the stock files from the clock (only after
#                                     a backup that verifies; ours are kept)
#   tools/stock_fs.sh restore dir     put the stock files back (removes desk.pak first
#                                     to make room); then flash the stock image
#
# Needs the clawd firmware's /api/fs/* endpoints. Clock address and password
# work as in tools/flash.sh (CLAWD_CLOCK, CLAWD_AUTH=user:pass).
# The backup holds config.json (WiFi password): .stock-backup/ is gitignored.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OURS='^/(config\.json|clawd\.pak|desk\.pak|.*\.tmp)$'   # files this firmware owns
FS_SIZE=3121152                                          # 4m3m LittleFS partition
FS_BLOCK=8192
FS_PAGE=256

clock=${CLAWD_CLOCK:-}
if [ -z "$clock" ] && [ -f "$HOME/.config/claude-usage-monitor/config" ]; then
  clock=$(sed -n 's/^[[:space:]]*http_targets[[:space:]]*=[[:space:]]*\([^,#[:space:]]*\).*/\1/p' \
    "$HOME/.config/claude-usage-monitor/config" | head -n 1)
fi
clock=${clock:-192.168.1.48}
case $clock in http://*|https://*) base=$clock ;; *) base="http://$clock" ;; esac
case $base in */api/*) base=${base%%/api/*} ;; esac

ccurl() {
  if [ -n "${CLAWD_AUTH:-}" ]; then curl -fsS --max-time 120 --digest -u "$CLAWD_AUTH" "$@"
  else curl -fsS --max-time 120 "$@"; fi
}
die() { echo "$*" >&2; exit 1; }
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

mklittlefs_bin() {
  for p in "$HOME"/.platformio/packages/tool-mklittlefs*/mklittlefs; do
    [ -x "$p" ] && { echo "$p"; return; }
  done
  command -v mklittlefs || die "mklittlefs not found: pio pkg install -g -t platformio/tool-mklittlefs"
}

backup() {
  dir=${1:-"$ROOT/.stock-backup/fs-$(date +%Y%m%d-%H%M%S)"}
  mkdir -p "$dir/files"
  echo "==> list $base/api/fs/list"
  ccurl "$base/api/fs/list" > "$dir/list.tsv"
  n=$(wc -l < "$dir/list.tsv" | tr -d ' ')
  [ "$n" -gt 0 ] || die "empty listing"
  echo "==> $n files"
  : > "$dir/sha256.tsv"
  while IFS="$(printf '\t')" read -r size path; do
    out="$dir/files$path"
    mkdir -p "$(dirname "$out")"
    ccurl -G --url-query "path=$path" "$base/api/fs/get" -o "$out"
    got=$(wc -c < "$out" | tr -d ' ')
    [ "$got" = "$size" ] || die "$path: got $got B, expected $size"
    printf '%s\t%s\n' "$(sha "$out")" "$path" >> "$dir/sha256.tsv"
    echo "    $size  $path"
  done < "$dir/list.tsv"
  echo "==> raw image $base/api/fs/raw"
  ccurl --max-time 600 "$base/api/fs/raw" -o "$dir/littlefs.bin"
  got=$(wc -c < "$dir/littlefs.bin" | tr -d ' ')
  [ "$got" = "$FS_SIZE" ] || die "raw image: got $got B, expected $FS_SIZE"
  echo "==> backup in $dir"
  verify "$dir"
}

verify() {
  dir=${1:?backup dir}
  [ -f "$dir/littlefs.bin" ] && [ -f "$dir/list.tsv" ] || die "$dir is not a backup"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  "$(mklittlefs_bin)" -u "$tmp/fs" -b "$FS_BLOCK" -p "$FS_PAGE" -s "$FS_SIZE" "$dir/littlefs.bin" >/dev/null
  bad=0
  while IFS="$(printf '\t')" read -r size path; do
    if printf '%s\n' "$path" | grep -Eq "$OURS"; then continue; fi   # ours may have changed meanwhile
    if ! cmp -s "$tmp/fs$path" "$dir/files$path"; then echo "    MISMATCH $path" >&2; bad=1; fi
  done < "$dir/list.tsv"
  [ $bad -eq 0 ] || die "raw image and downloaded files differ"
  touch "$dir/VERIFIED"
  echo "==> verified: every stock file in the raw image matches its download"
}

delete() {
  dir=${1:?backup dir}
  [ -f "$dir/VERIFIED" ] || die "$dir has no verified backup: run backup/verify first"
  now=$(mktemp)
  ccurl "$base/api/fs/list" > "$now"
  freed=0
  while IFS="$(printf '\t')" read -r size path; do
    if printf '%s\n' "$path" | grep -Eq "$OURS"; then continue; fi
    grep -qF "$(printf '\t%s' "$path")" "$dir/list.tsv" || die "$path is on the clock but not in the backup"
    ccurl -X POST -G --url-query "path=$path" "$base/api/fs/rm" >/dev/null || die "rm $path failed"
    freed=$((freed + size))
    echo "    rm $path"
  done < "$now"
  rm -f "$now"
  echo "==> deleted ~$freed B of stock files"
  ccurl "$base/api/heap" | sed -n 's/.*"fsTotal":\([0-9]*\),"fsUsed":\([0-9]*\).*/    fs: \2 of \1 B used/p'
}

restore() {
  dir=${1:?backup dir}
  [ -f "$dir/VERIFIED" ] || die "$dir has no verified backup"
  echo "==> removing /desk.pak to make room"
  ccurl -X POST -G --url-query "path=/desk.pak" "$base/api/fs/rm" >/dev/null 2>&1 || true
  while IFS="$(printf '\t')" read -r size path; do
    if printf '%s\n' "$path" | grep -Eq "$OURS"; then continue; fi
    ccurl -G --url-query "path=$path" -F "file=@$dir/files$path;filename=x" "$base/api/fs/put" >/dev/null \
      || die "put $path failed"
    echo "    put $path"
  done < "$dir/list.tsv"
  echo "==> stock files restored; check with: tools/stock_fs.sh check $dir"
}

# Compare what is on the clock with a backup (sizes only, no download).
check() {
  dir=${1:?backup dir}
  now=$(mktemp)
  ccurl "$base/api/fs/list" > "$now"
  missing=0
  while IFS="$(printf '\t')" read -r size path; do
    if printf '%s\n' "$path" | grep -Eq "$OURS"; then continue; fi
    grep -qxF "$(printf '%s\t%s' "$size" "$path")" "$now" || { echo "    missing/changed $path"; missing=1; }
  done < "$dir/list.tsv"
  rm -f "$now"
  [ $missing -eq 0 ] && echo "==> every stock file is on the clock" || exit 1
}

cmd=${1:-}
[ $# -gt 0 ] && shift
case $cmd in
  backup) backup "$@" ;;
  verify) verify "$@" ;;
  delete) delete "$@" ;;
  restore) restore "$@" ;;
  check) check "$@" ;;
  *) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
