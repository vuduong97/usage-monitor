#!/bin/sh
# Link (or unlink with --uninstall) the clawd-home pi extension into pi's
# personal extensions directory. A symlink, so repo updates apply on the next
# pi start / reload. Existing files that are not our link are never touched.
set -eu
src=$(cd "$(dirname "$0")" && pwd)/clawd-home.ts
dir=${PI_EXTENSIONS_DIR:-"$HOME/.pi/agent/extensions"}
dst="$dir/clawd-home.ts"
if [ "${1:-}" = "--uninstall" ]; then
  if [ -L "$dst" ]; then rm "$dst"; echo "removed $dst"; else echo "nothing to remove"; fi
  exit 0
fi
mkdir -p "$dir"
if [ -e "$dst" ] && [ ! -L "$dst" ]; then
  echo "$dst exists and is not a symlink; leaving it alone" >&2
  exit 1
fi
ln -sfn "$src" "$dst"
echo "linked $dst -> $src (restart pi or /reload)"
