#!/bin/sh
# Publish the usage style packs of clock/firmware/styles to the GitHub Release packs-api<N> of
# this repo, signed with the app's minisign key. Installed apps (0.3.0+) whose bundled firmware
# draws style API <N> pick them up the next time the settings window opens, when the release's
# rev is higher than the packs they carry. Firmware is never published this way: it only ships
# with an app release.
#
#   sh tools/publish_packs.sh                 # stage + sign into build/packs, publish nothing
#   sh tools/publish_packs.sh --publish       # ... then create/refresh the release
#
# Signing: MINISIGN_KEY_FILE (default ~/.minisign/clawd.key) is the secret key; minisign asks
# for its password, or reads it from MINISIGN_PASSWORD (CI). MINISIGN_PUBKEY (the base64 line,
# the repository variable) verifies the signature before anything is uploaded.
#
# The release is created with --latest=false and never becomes "latest": apps 0.1.x/0.2.x
# update themselves from /releases/latest and must keep finding a v* tag there (checked after
# every publish). Its rev must be higher than the published one (--force to republish).
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC="$ROOT/clock/firmware"
OUT="$ROOT/build/packs"
REPO=${CLAWD_REPO:-vuduong97/usage-monitor}
PUBLISH=0
FORCE=0
REV=""
while [ $# -gt 0 ]; do
  case "$1" in
    --publish) PUBLISH=1 ;;
    --force) FORCE=1 ;;
    --src) SRC=$2; shift ;;
    --out) OUT=$2; shift ;;
    --rev) REV=$2; shift ;;
    --repo) REPO=$2; shift ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "publish_packs: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done

die() { echo "publish_packs: $*" >&2; exit 1; }
command -v minisign >/dev/null || die "minisign is not installed (brew install minisign)"

set -- --src "$SRC" --out "$OUT"
[ -n "$REV" ] && set -- "$@" --rev "$REV"
STAGED=$(python3 "$ROOT/tools/publish_packs.py" "$@") || exit 1
read -r API REV COUNT <<EOF
$STAGED
EOF
TAG="packs-api$API"
echo "staged $COUNT style packs for $TAG, rev $REV -> $OUT"

KEY=${MINISIGN_KEY_FILE:-$HOME/.minisign/clawd.key}
[ -f "$KEY" ] || die "no secret key at $KEY (set MINISIGN_KEY_FILE)"
rm -f "$OUT/index.json.minisig"
if [ -n "${MINISIGN_PASSWORD:-}" ]; then
  printf '%s\n' "$MINISIGN_PASSWORD" |
    minisign -S -s "$KEY" -m "$OUT/index.json" -t "Clawd $TAG rev $REV"
else
  minisign -S -s "$KEY" -m "$OUT/index.json" -t "Clawd $TAG rev $REV"
fi
if [ -n "${MINISIGN_PUBKEY:-}" ]; then
  minisign -V -q -P "$MINISIGN_PUBKEY" -m "$OUT/index.json" ||
    die "the signature does not verify with MINISIGN_PUBKEY: wrong key?"
else
  echo "warning: MINISIGN_PUBKEY not set, the signature was not checked against the app's key" >&2
fi

[ "$PUBLISH" = 1 ] || { echo "not published (pass --publish)"; exit 0; }
command -v gh >/dev/null || die "gh is not installed"

# rev must go up: an app keeps the highest rev it has seen as the newer one
if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  PUBLISHED=$(gh release download "$TAG" --repo "$REPO" -p index.json -O - 2>/dev/null |
    python3 -c 'import json,sys; print(json.load(sys.stdin).get("rev", 0))' 2>/dev/null || echo 0)
  if [ "$REV" -le "$PUBLISHED" ] && [ "$FORCE" != 1 ]; then
    die "$TAG already has rev $PUBLISHED (>= $REV): change the packs, or --force"
  fi
  echo "refreshing $TAG (rev $PUBLISHED -> $REV)"
  # packs first, then the index and its signature in one call: an app never sees an index
  # that names a pack the release does not hold yet
  (cd "$OUT" && gh release upload "$TAG" --repo "$REPO" --clobber ./*.pak)
  (cd "$OUT" && gh release upload "$TAG" --repo "$REPO" --clobber index.json index.json.minisig)
  # drop packs the new index no longer lists
  KEEP=$(cd "$OUT" && ls ./*.pak | sed 's|^\./||')
  gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[].name' |
    while read -r name; do
      case "$name" in *.pak) ;; *) continue ;; esac
      echo "$KEEP" | grep -qx "$name" || gh release delete-asset "$TAG" "$name" --repo "$REPO" -y
    done
  gh release edit "$TAG" --repo "$REPO" --latest=false --title "Usage screens (style API $API)" >/dev/null
else
  echo "creating $TAG"
  (cd "$OUT" && gh release create "$TAG" --repo "$REPO" --latest=false \
    --title "Usage screens (style API $API)" \
    --notes "Usage screen style packs for Clawd's clock firmware (style API $API), signed with the app's release key. Apps pick them up by themselves; nothing to download here." \
    ./*.pak index.json index.json.minisig)
fi

LATEST=$(gh api "repos/$REPO/releases/latest" --jq .tag_name 2>/dev/null || echo "")
case "$LATEST" in
  v*) echo "published $TAG rev $REV; releases/latest is still $LATEST" ;;
  *) die "releases/latest is '$LATEST', not a v* tag: apps 0.1.x/0.2.x would stop updating. Fix it now: gh release edit <vX.Y.Z> --latest" ;;
esac
