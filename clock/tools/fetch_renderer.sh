#!/bin/sh
# Build the desktop app's clock-screen renderer for this commit on GitHub Actions
# (.github/workflows/renderer.yml: Windows + macOS) and download the zips into
# build/renderer/, where tools/release.sh picks them up.
#
#   tools/fetch_renderer.sh            HEAD (must be pushed)
#   tools/fetch_renderer.sh RUN_ID     download the zips of a finished run
#
# Env: DESKTOP_REF (default main): the clawd-desktop (usage-monitor) ref to build with.
# Needs gh (logged in, with access to this repo).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/build/renderer"
wf=renderer.yml

if [ $# -gt 0 ]; then
  run=$1
else
  head=$(git -C "$root" rev-parse HEAD)
  branch=$(git -C "$root" rev-parse --abbrev-ref HEAD)
  [ "$branch" != HEAD ] || { echo "fetch_renderer: check out a branch (the workflow runs on one)" >&2; exit 1; }
  git -C "$root" fetch -q origin "$branch"
  [ "$(git -C "$root" rev-parse "origin/$branch")" = "$head" ] ||
    { echo "fetch_renderer: push $branch first (the workflow builds what GitHub has)" >&2; exit 1; }
  gh workflow run "$wf" --ref "$branch" -f desktop_ref="${DESKTOP_REF:-main}"
  echo "waiting for the run of $head..."
  run=""
  for _ in $(seq 1 30); do
    sleep 4
    run=$(gh run list --workflow "$wf" --branch "$branch" --event workflow_dispatch --limit 5 \
            --json databaseId,headSha --jq ".[] | select(.headSha == \"$head\") | .databaseId" | head -n 1)
    [ -n "$run" ] && break
  done
  [ -n "$run" ] || { echo "fetch_renderer: the run did not show up" >&2; exit 1; }
  gh run watch "$run" --exit-status
fi

mkdir -p "$out"
for p in win-x64 mac; do
  rm -f "$out/$p.zip"
  gh run download "$run" -n "renderer-$p" -D "$out"
done
ls -l "$out"/*.zip
