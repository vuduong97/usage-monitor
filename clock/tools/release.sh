#!/bin/sh
# Publish the firmware + packs to the Clawd firmware feed (feed/README.md).
#
#   tools/release.sh [--notes "..."]        build, encrypt, upload; publish to the beta channel
#   tools/release.sh --stable [--notes ..]  same, straight to beta and stable
#   tools/release.sh --promote [SEQ]        copy the beta release (or release SEQ) to stable
#
# Options: --dirty (allow uncommitted changes), --skip-tests, --local DIR (use the
# wrangler dev store in DIR instead of Cloudflare: for the end-to-end test),
# --private (publish to Cloudflare without feed/LICENCE: only while no public app holds
# the feed token, i.e. the owner's own test).
# Env: DESK_PAK (default build/desk/desk.pak), CLAWD_PAK (default firmware/data/clawd.pak),
#      FEED_BUCKET (default clawd-fw), WRANGLER (default "npx -y wrangler@4").
#
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
fw="$root/firmware"
worker="$root/feed/worker"
bucket=${FEED_BUCKET:-clawd-fw}
WRANGLER=${WRANGLER:-"npx -y wrangler@4"}
DESK_PAK=${DESK_PAK:-"$root/build/desk/desk.pak"}
CLAWD_PAK=${CLAWD_PAK:-"$fw/data/clawd.pak"}

notes="" stable=0 promote=0 pseq="" dirty=0 tests=1 where="--remote" private=0
while [ $# -gt 0 ]; do
  case "$1" in
    --notes) notes=$2; shift ;;
    --stable) stable=1 ;;
    --promote) promote=1; case "${2:-}" in [0-9]*) pseq=$2; shift ;; esac ;;
    --dirty) dirty=1 ;;
    --skip-tests) tests=0 ;;
    --private) private=1 ;;
    --local) where="--local --persist-to $2"; shift ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "release: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done

# The feed hands the art and fonts to anyone with the app: no real publish without the
# licence that allows it recorded in feed/LICENCE (a line "Feed licence: <owner, scope, date>").
if [ "$where" = "--remote" ] && ! grep -qs '^Feed licence: ..' "$root/feed/LICENCE"; then
  if [ $private = 1 ]; then
    echo "release: PRIVATE publish without feed/LICENCE: do not ship the feed token in a public app" >&2
  else
    echo "release: feed/LICENCE has no 'Feed licence:' line; record the distribution licence first" >&2
    echo "         (or --private for the owner's own test, while no public app holds the token)" >&2
    exit 1
  fi
fi

wr() { (cd "$worker" && $WRANGLER "$@"); }
# A channel's published seq, 0 when it has none yet.
channel_seq() {
  wr r2 object get "$bucket/channels/$1.json" --pipe $where 2>/dev/null |
    node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{try{console.log(JSON.parse(s).seq|0)}catch{console.log(0)}})'
}
put() {   # put <key> <file> <content-type>
  wr r2 object put "$bucket/$1" --file "$2" --content-type "$3" $where >/dev/null
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [ $promote = 1 ]; then
  [ -n "$pseq" ] || pseq=$(channel_seq beta)
  [ "$pseq" -gt 0 ] || { echo "release: nothing on the beta channel to promote" >&2; exit 1; }
  wr r2 object get "$bucket/rel/$pseq/manifest.json" --pipe $where > "$tmp/m.json" 2>/dev/null ||
    { echo "release: no release $pseq" >&2; exit 1; }
  cur=$(channel_seq stable)
  [ "$pseq" -gt "$cur" ] || { echo "release: stable is already at $cur" >&2; exit 1; }
  put channels/stable.json "$tmp/m.json" application/json
  echo "stable -> release $pseq"
  exit 0
fi

if [ $dirty = 0 ] && [ -n "$(git -C "$root" status --porcelain --untracked-files=no)" ]; then
  echo "release: uncommitted changes (commit them, or --dirty)" >&2; exit 1
fi
seq=$(node "$root/feed/release.mjs" seq --src "$fw")
published=$(channel_seq beta)
st=$(channel_seq stable)
[ "$st" -gt "$published" ] && published=$st
if [ "$seq" -le "$published" ]; then
  echo "release: CLAWD_FW_SEQ is $seq but $published is already published: bump it in firmware/src/config.h" >&2
  exit 1
fi
[ -f "$DESK_PAK" ] || { echo "release: no $DESK_PAK (build it with tools/desk, or set DESK_PAK)" >&2; exit 1; }

echo "== build fwSeq $seq"
(cd "$fw" && pio run -e clawd -s)
if [ $tests = 1 ]; then
  echo "== tests"
  sh "$fw/tests/host/run.sh"
  node --test "$worker"/test/*.test.mjs >/dev/null
  # run.sh ends in a pipe, so its exit status says nothing: read the checks' own lines
  DESK_PACK_PATH="$DESK_PAK" sh "$fw/tools/clawd_preview/run.sh" "$tmp/preview" > "$tmp/preview.log" 2>&1 || true
  if grep -q '^FAIL' "$tmp/preview.log" || ! grep -q '^variants: ok' "$tmp/preview.log"; then
    grep '^FAIL' "$tmp/preview.log" | head -20; echo "release: preview checks failed" >&2; exit 1
  fi
fi

echo "== pack"
commit=$(git -C "$root" rev-parse --short HEAD)
[ $dirty = 1 ] && [ -n "$(git -C "$root" status --porcelain --untracked-files=no)" ] && commit="$commit-dirty"
node "$root/feed/release.mjs" pack --src "$fw" --fw "$fw/.pio/build/clawd/firmware.bin" \
  --clawd "$CLAWD_PAK" --desk "$DESK_PAK" --out "$tmp/out" --notes "$notes" --commit "$commit" > "$tmp/files"

echo "== upload ($(wc -l < "$tmp/files" | tr -d ' ') files)"
while read -r f; do put "$f" "$tmp/out/$f" application/octet-stream; done < "$tmp/files"
wr kv key put --binding KEYS "key:$seq" --path "$tmp/out/key.b64" $where >/dev/null
# The manifest last: a client only ever sees a release whose files are all up.
put "rel/$seq/manifest.json" "$tmp/out/rel/$seq/manifest.json" application/json
put channels/beta.json "$tmp/out/rel/$seq/manifest.json" application/json
[ $stable = 1 ] && put channels/stable.json "$tmp/out/rel/$seq/manifest.json" application/json
echo "published release $seq ($commit) to beta$([ $stable = 1 ] && echo ' and stable')"
