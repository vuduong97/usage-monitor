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
#   CLAWD_EXPECTED_TEAM_ID   the Team ID of the 0.1.x Developer ID (workflow variable EXPECTED_TEAM_ID):
#                         the signed app must carry exactly this TeamIdentifier. Required when
#                         CLAWD_REQUIRE_SIGNING=1.
#   CLAWD_EXPECT_ARCH     fail unless `uname -m` is this (the workflow's runner label check)
#   CLAWD_MINISIGN_PUBKEY   baked into the binary by cargo (app updates and remote style packs)
#
# The clock's firmware ships inside the app: build/firmware (image, clawd.pak, desk.pak, style
# packs) and build/renderer (the clock-screen renderer), both from `sh tools/build_clock.sh`
# (CI: the firmware job's artifact + tools/build_clock.sh --renderer). A release (signed, or any
# CI run) without them fails; a local build without them only warns.
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
  [ -n "${CLAWD_EXPECTED_TEAM_ID:-}" ] || die "a release build needs CLAWD_EXPECTED_TEAM_ID (the 0.1.x Team ID, repository variable EXPECTED_TEAM_ID)"
fi
NEED_FW=0
if [ "$REQUIRE" = 1 ] || [ "${GITHUB_ACTIONS:-}" = true ]; then NEED_FW=1; fi
if [ ! -f build/firmware/firmware.json ] || [ ! -f build/renderer/preview.json ]; then
  [ "$NEED_FW" = 0 ] || die "no build/firmware or build/renderer: run sh tools/build_clock.sh first (a release always carries the clock firmware)"
  echo "build_mac.sh: no build/firmware or build/renderer (sh tools/build_clock.sh): this app carries no clock firmware" >&2
fi

VERSION=$(python3 packaging/check_version.py) || die "Cargo.toml and tauri.conf.json disagree on the version"
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}

# The bundler signs and notarizes by itself when it sees these; this script does both, explicitly,
# below, so the bundler only assembles the .app (ad-hoc at most).
rm -rf "$TARGET/release/bundle/macos/Clawd.app" dist/Clawd.app
# With a committed Cargo.lock the build must not change it. `cargo fetch --locked` fails when the
# lock is out of date (and fetches the crates); the arguments after `--` are what the Tauri CLI
# hands on to cargo (not verified on a Mac: if it does not forward them, the fetch above still holds).
LOCKED=
if [ -f Cargo.lock ]; then
  LOCKED=--locked
  cargo fetch --locked
fi
(cd app && env -u APPLE_SIGNING_IDENTITY -u APPLE_CERTIFICATE -u APPLE_CERTIFICATE_PASSWORD \
  -u APPLE_ID -u APPLE_PASSWORD -u APPLE_TEAM_ID -u APPLE_API_KEY -u APPLE_API_ISSUER -u APPLE_API_KEY_PATH \
  cargo tauri build --bundles app ${LOCKED:+-- $LOCKED})
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

if [ -f build/firmware/firmware.json ]; then
  python3 tools/bundle_firmware.py --check build/firmware || die "build/firmware does not check out"
  cp -R build/firmware "$APP/Contents/Resources/firmware"
  if [ -f build/renderer/preview.json ]; then
    cp -R build/renderer "$APP/Contents/Resources/renderer"
  fi
  echo "bundled the clock firmware: $(python3 -c 'import json;m=json.load(open("build/firmware/firmware.json"));print("fwSeq",m["fwSeq"],"styleApi",m["styleApi"],"commit",m["commit"])')"
fi

if [ -n "$SIGN" ]; then
  codesign --force --timestamp --options runtime --entitlements packaging/entitlements.plist \
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
  # Notarization needs the hardened runtime: `flags=0x10000(runtime)`.
  echo "$INFO" | grep -q 'flags=.*runtime' || die "the hardened runtime flag is missing from the signature"
  TEAM=$(echo "$INFO" | sed -n 's/^TeamIdentifier=//p')
  [ -n "$TEAM" ] && [ "$TEAM" != "not set" ] || die "the app has no TeamIdentifier"
  [ -z "${APPLE_TEAM_ID:-}" ] || [ "$TEAM" = "$APPLE_TEAM_ID" ] || die "TeamIdentifier $TEAM != APPLE_TEAM_ID"
  [ -z "${CLAWD_EXPECTED_TEAM_ID:-}" ] || [ "$TEAM" = "$CLAWD_EXPECTED_TEAM_ID" ] || die "TeamIdentifier $TEAM != the expected 0.1.x Team ID $CLAWD_EXPECTED_TEAM_ID"
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
