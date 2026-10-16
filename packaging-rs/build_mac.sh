#!/bin/sh
# Build dist/Clawd.app, dist/Clawd-<version>-mac-<arch>.zip (what the app's self-update downloads)
# and dist/Clawd-<version>-mac-<arch>.dmg (first install) for the machine's own architecture
# (arm64 or x86_64). Run on macOS. Needs the Tauri CLI (cargo install tauri-cli --version "^2" --locked).
#
# Signing is chosen by the environment (the release workflow sets these from its secrets):
#   CLAWD_SIGN_IDENTITY   "Developer ID Application: Name (TEAMID)": sign with the hardened runtime
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD   also notarize and staple (needs the identity)
#   CLAWD_REQUIRE_SIGNING=1   (set automatically when GITHUB_REF_TYPE=tag) a release must be signed
#                             with the Developer ID AND notarized: anything missing is an error.
#                             Without it, a build with no identity gets an ad-hoc signature: it runs
#                             on this Mac, but it is not a release and 0.1.x will not update to it.
#   CLAWD_EXPECT_ARCH     fail unless `uname -m` is this (the workflow's runner label check)
#   CLAWD_PRIVATE_FIRMWARE=1   also ship clawd_desktop/firmware and /renderer (clock owners only)
#   CLAWD_FEED_URL, CLAWD_FEED_TOKEN, CLAWD_MINISIGN_PUBKEY   baked into the binary by cargo
set -eu
cd "$(dirname "$0")/.."
ROOT=$(pwd)
die() { echo "build_mac.sh: $*" >&2; exit 1; }

ARCH=$(uname -m)
[ -z "${CLAWD_EXPECT_ARCH:-}" ] || [ "$ARCH" = "$CLAWD_EXPECT_ARCH" ] || die "this machine is $ARCH, expected $CLAWD_EXPECT_ARCH"
case "$ARCH" in arm64|x86_64) ;; *) die "unsupported architecture $ARCH" ;; esac

REQUIRE=${CLAWD_REQUIRE_SIGNING:-0}
[ "${GITHUB_REF_TYPE:-}" != tag ] || REQUIRE=1
SIGN=${CLAWD_SIGN_IDENTITY:-}
NOTARIZE=0
if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then NOTARIZE=1; fi
if [ "$REQUIRE" = 1 ]; then
  [ -n "$SIGN" ] || die "a release build needs CLAWD_SIGN_IDENTITY (Developer ID Application, the same Team ID as 0.1.x); refusing an ad-hoc release"
  [ "$NOTARIZE" = 1 ] || die "a release build must be notarized: APPLE_ID, APPLE_TEAM_ID and APPLE_APP_PASSWORD are all required"
fi
if [ "${CLAWD_PRIVATE_FIRMWARE:-0}" = 1 ] && [ "${GITHUB_ACTIONS:-}" = true ]; then
  die "CLAWD_PRIVATE_FIRMWARE=1 is not allowed in CI: public artifacts carry no firmware"
fi

VERSION=$(python3 packaging-rs/check_version.py) || die "Cargo.toml and tauri.conf.json disagree on the version"
TARGET=${CARGO_TARGET_DIR:-$ROOT/desktop-rs/target}

# The bundler signs and notarizes by itself when it sees these; this script does both, explicitly,
# below, so the bundler only assembles the .app (ad-hoc at most).
rm -rf "$TARGET/release/bundle/macos/Clawd.app" dist/Clawd.app
(cd desktop-rs/app && env -u APPLE_SIGNING_IDENTITY -u APPLE_CERTIFICATE -u APPLE_CERTIFICATE_PASSWORD \
  -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH \
  cargo tauri build --bundles app)
BUILT="$TARGET/release/bundle/macos/Clawd.app"
[ -x "$BUILT/Contents/MacOS/Clawd" ] || die "no $BUILT/Contents/MacOS/Clawd (the binary must be named Clawd)"
mkdir -p dist
ditto "$BUILT" dist/Clawd.app
APP=dist/Clawd.app
PLIST="$APP/Contents/Info.plist"

# What 0.1.x and macOS expect of the bundle (the Python bundle had the same values).
plist() { /usr/libexec/PlistBuddy -c "Print :$1" "$PLIST"; }
[ "$(plist CFBundleIdentifier)" = vn.clawd.desktop ] || die "CFBundleIdentifier is not vn.clawd.desktop"
[ "$(plist CFBundleExecutable)" = Clawd ] || die "CFBundleExecutable is not Clawd"
[ "$(plist CFBundleShortVersionString)" = "$VERSION" ] || die "CFBundleShortVersionString is not $VERSION"
[ "$(plist LSUIElement)" = true ] || die "LSUIElement is not true"
[ "$(plist LSMinimumSystemVersion)" = 11.0 ] || die "LSMinimumSystemVersion is not 11.0"
[ "$(plist NSBonjourServices:0)" = _clawd._tcp ] || die "NSBonjourServices lacks _clawd._tcp"
plist NSLocalNetworkUsageDescription >/dev/null || die "NSLocalNetworkUsageDescription is missing"

if [ "${CLAWD_PRIVATE_FIRMWARE:-0}" = 1 ]; then
  [ -f clawd_desktop/firmware/firmware.json ] || die "no firmware bundle in clawd_desktop/firmware (tools/bundle_firmware.py)"
  cp -R clawd_desktop/firmware "$APP/Contents/Resources/firmware"
  if [ -f clawd_desktop/renderer/preview.json ]; then
    cp -R clawd_desktop/renderer "$APP/Contents/Resources/renderer"
  else
    echo "no clock-screen renderer built (tools/build_preview.py); the preview will be approximate" >&2
  fi
fi

if [ -n "$SIGN" ]; then
  codesign --force --timestamp --options runtime --entitlements packaging-rs/entitlements.plist \
    --sign "$SIGN" "$APP"
else
  # Ad-hoc, as before: an unsigned bundle does not run on Apple silicon at all.
  echo "ad-hoc signature (not a release)" >&2
  codesign --force --sign - "$APP"
fi
codesign --verify --deep --strict --verbose=2 "$APP"
INFO=$(codesign -dv --verbose=2 "$APP" 2>&1)
echo "$INFO" | grep -E '^(Authority|TeamIdentifier)=' | head -2
if [ -n "$SIGN" ]; then
  TEAM=$(echo "$INFO" | sed -n 's/^TeamIdentifier=//p')
  [ -n "$TEAM" ] && [ "$TEAM" != "not set" ] || die "the app has no TeamIdentifier"
  [ -z "${APPLE_TEAM_ID:-}" ] || [ "$TEAM" = "$APPLE_TEAM_ID" ] || die "TeamIdentifier $TEAM != APPLE_TEAM_ID"
fi

if [ "$NOTARIZE" = 1 ] && [ -n "$SIGN" ]; then
  mkdir -p build
  ditto -c -k --keepParent "$APP" build/notarize.zip
  xcrun notarytool submit build/notarize.zip --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_PASSWORD" --wait 2>&1 | tee build/notarize.log
  grep -q "status: Accepted" build/notarize.log || die "notarization was not accepted"
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"
  spctl --assess --type execute --verbose=2 "$APP"
elif [ -n "$SIGN" ]; then
  echo "signed but NOT notarized (APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD not all set)" >&2
fi

STAGE=build/dmg
rm -rf "$STAGE"; mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="dist/Clawd-$VERSION-mac-$ARCH.dmg"
rm -f "$DMG"
hdiutil create -quiet -volname "Clawd" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
echo "built $DMG"
# The self-update's file: the stapled app, zipped the way Finder does (keeps the signature).
ZIP="dist/Clawd-$VERSION-mac-$ARCH.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
echo "built $ZIP"
echo "Clawd binary: $(stat -f %z "$APP/Contents/MacOS/Clawd") bytes (reported only, not gated)"
