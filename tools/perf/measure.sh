#!/usr/bin/env bash
# Repeatable performance measurement of the Tauri spike (or any tray exe) on macOS and Linux.
# Windows: use measure.ps1 (it can also measure the Python app and sees real paint from outside).
#
#   tools/perf/measure.sh --exe path/to/clawd-spike [--installer path] [--runs 3] [--cycles 50]
#                         [--idle-seconds 600] [--gap-ms 300] [--out result.json]
#
# Prints one JSON document {installer_bytes, idle_private_mb_10s, idle_private_mb_10m, tray_ms,
# paint_cold_ms, paint_warm_ms, rss_after_50_cycles_mb, ...}: medians over the runs, raw values in "runs".
#
# Differences from measure.ps1 (macOS has no cheap, permission-free way to read another window's pixels):
#   * tray_ms   = spike's own tray_ready stamp - launch (the spike logs it with CLAWD_SPIKE_LOG);
#   * paint_*   = spike's own "shown" stamp (after the page's DOMContentLoaded + 2 animation frames)
#                 - open_requested stamp. This is a few ms earlier than a pixel on screen.
#   * memory    = macOS: `footprint` physical footprint (closest to "private"), else RSS; Linux:
#                 Private_Clean+Private_Dirty from /proc/PID/smaps_rollup.
# The process runs with HOME and TMPDIR pointed at a throw-away directory and token-looking
# environment variables removed, so it never sees real credentials.
#
# NOT EXECUTED ON macOS YET: written and syntax-checked on Windows (Git Bash); macOS numbers are
# deferred to phase 14.
set -euo pipefail

exe="" installer="" runs=3 cycles=50 idle_seconds=600 gap_ms=300 out="" base_port=47960
while [ $# -gt 0 ]; do
  case "$1" in
    --exe) exe="$2"; shift 2 ;;
    --installer) installer="$2"; shift 2 ;;
    --runs) runs="$2"; shift 2 ;;
    --cycles) cycles="$2"; shift 2 ;;
    --idle-seconds) idle_seconds="$2"; shift 2 ;;
    --gap-ms) gap_ms="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --base-port) base_port="$2"; shift 2 ;;
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$exe" ] && [ -x "$exe" ] || { echo "--exe must point to an executable" >&2; exit 2; }

os="$(uname -s)"
now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time()*1000'; }
file_bytes() { wc -c < "$1" | tr -d ' '; }
sleep_ms() { perl -MTime::HiRes=usleep -e "usleep($1*1000)"; }

# private MB of a pid
private_mb() {
  local pid="$1" kb=0
  if [ "$os" = "Darwin" ] && command -v footprint >/dev/null 2>&1; then
    # "Footprint: 12 MB" / "Footprint: 800 KB"
    footprint -p "$pid" 2>/dev/null | awk '/Footprint:/ { v=$2; u=$3; if (u=="KB") v=v/1024; else if (u=="GB") v=v*1024; printf "%.2f\n", v; exit }'
    return
  fi
  if [ -r "/proc/$pid/smaps_rollup" ]; then
    kb=$(awk '/^Private_(Clean|Dirty):/ { s += $2 } END { print s+0 }' "/proc/$pid/smaps_rollup")
  else
    kb=$(ps -o rss= -p "$pid" | tr -d ' ')
  fi
  awk -v kb="$kb" 'BEGIN { printf "%.2f\n", kb/1024 }'
}

median() { sort -g | awk '{ a[NR]=$1 } END { if (NR==0) { print "null"; exit } if (NR%2) print a[(NR+1)/2]; else printf "%.2f\n", (a[NR/2]+a[NR/2+1])/2 }'; }
http() { curl -s -o /dev/null --max-time 20 "http://127.0.0.1:$1$2" || true; }
log_ts() { awk -v ev="$2" '$2==ev { print $1 }' "$1" 2>/dev/null; }              # all stamps of an event
log_nth() { log_ts "$1" "$2" | sed -n "${3}p"; }                         # n-th (1-based) stamp

tray_all="" cold_all="" warm_all="" idle10_all="" idle10m_all="" cycles_all="" run_json=""
for i in $(seq 1 "$runs"); do
  work="$(mktemp -d "${TMPDIR:-/tmp}/clawd-perf.XXXXXX")"
  mkdir -p "$work/home" "$work/tmp"
  port=$((base_port + i))
  logfile="$work/spike.log"
  t_launch=$(now_ms)
  env -i PATH="$PATH" HOME="$work/home" TMPDIR="$work/tmp" \
    CLAWD_SPIKE_PORT="$port" CLAWD_SPIKE_CONTROL=1 CLAWD_SPIKE_LOG="$logfile" \
    "$exe" >"$work/stdout.log" 2>&1 &
  pid=$!
  trap 'kill "$pid" 2>/dev/null || true' EXIT

  for _ in $(seq 1 400); do [ -n "$(log_ts "$logfile" tray_ready 2>/dev/null)" ] && break; sleep_ms 25; done
  tray_ms=$(awk -v a="$(log_nth "$logfile" tray_ready 1)" -v b="$t_launch" 'BEGIN { print a-b }')

  # idle samples are taken from launch time, with no window ever opened
  wait_until() { local target=$(( t_launch + $1 * 1000 )) n; n=$(now_ms); [ "$n" -lt "$target" ] && sleep_ms $(( target - n )); true; }
  wait_until 10
  idle10=$(private_mb "$pid")
  idle10m="null"
  if [ "$idle_seconds" -gt 10 ]; then wait_until "$idle_seconds"; idle10m=$(private_mb "$pid"); fi

  # cold, then the cycles (warm)
  http "$port" /open
  for _ in $(seq 1 300); do [ -n "$(log_nth "$logfile" shown 1)" ] && break; sleep_ms 25; done
  cold_ms=$(awk -v a="$(log_nth "$logfile" shown 1)" -v b="$(log_nth "$logfile" open_requested 1)" 'BEGIN { print a-b }')
  sleep_ms 1500
  http "$port" /close
  sleep_ms 3000

  warm_list=""
  for c in $(seq 1 "$cycles"); do
    http "$port" /open
    for _ in $(seq 1 300); do [ -n "$(log_nth "$logfile" shown $((c + 1)))" ] && break; sleep_ms 25; done
    ms=$(awk -v a="$(log_nth "$logfile" shown $((c + 1)))" -v b="$(log_nth "$logfile" open_requested $((c + 1)))" 'BEGIN { print a-b }')
    warm_list="$warm_list$ms"$'\n'
    sleep_ms 800
    http "$port" /close
    sleep_ms "$gap_ms"
  done
  sleep_ms 5000
  after=$(private_mb "$pid")
  warm_ms=$(printf '%s' "$warm_list" | sed '/^$/d' | median)

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  trap - EXIT
  rm -rf "$work"

  tray_all="$tray_all$tray_ms"$'\n'; cold_all="$cold_all$cold_ms"$'\n'; warm_all="$warm_all$warm_ms"$'\n'
  idle10_all="$idle10_all$idle10"$'\n'; cycles_all="$cycles_all$after"$'\n'
  [ "$idle10m" != "null" ] && idle10m_all="$idle10m_all$idle10m"$'\n'
  run_json="$run_json${run_json:+,}{\"run\":$i,\"tray_ms\":$tray_ms,\"idle_private_mb_10s\":$idle10,\"idle_private_mb_10m\":$idle10m,\"paint_cold_ms\":$cold_ms,\"paint_warm_ms\":$warm_ms,\"rss_after_cycles_mb\":$after}"
done

med() { printf '%s' "$1" | sed '/^$/d' | median; }
installer_bytes="null"; [ -n "$installer" ] && installer_bytes=$(file_bytes "$installer")
json=$(printf '{"schema":1,"kind":"tauri-spike","os":"%s","cycles":%s,"idle_seconds":%s,"installer_bytes":%s,"exe_bytes":%s,"idle_private_mb_10s":%s,"idle_private_mb_10m":%s,"tray_ms":%s,"paint_cold_ms":%s,"paint_warm_ms":%s,"rss_after_50_cycles_mb":%s,"runs":[%s]}' \
  "$os" "$cycles" "$idle_seconds" "$installer_bytes" "$(file_bytes "$exe")" \
  "$(med "$idle10_all")" "$(med "$idle10m_all")" "$(med "$tray_all")" "$(med "$cold_all")" "$(med "$warm_all")" "$(med "$cycles_all")" "$run_json")
[ -n "$out" ] && printf '%s\n' "$json" > "$out"
printf '%s\n' "$json"
