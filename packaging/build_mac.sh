#!/bin/sh
# Build dist/Clawd.app, dist/Clawd-<version>-mac-<arch>.dmg (first install) and
# dist/Clawd-<version>-mac-<arch>.zip (what the app's self-update downloads). Run on macOS.
#
# Signing is opt-in through the environment (the release workflow sets these from its secrets):
#   CLAWD_SIGN_IDENTITY   "Developer ID Application: Name (TEAMID)": sign with the hardened runtime
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD   also notarize and staple (needs the identity)
# Without them the app gets an ad-hoc signature: it runs, but macOS asks the user to open it once.
set -eu
cd "$(dirname "$0")/.."
PY=${PYTHON:-3.12}
uv sync -q --python "$PY" --group build
uv run -q --python "$PY" python packaging/make_icons.py
rm -rf build/Clawd dist/Clawd dist/Clawd.app
uv run -q --python "$PY" --group build pyinstaller --noconfirm --clean --log-level WARN \
  --distpath dist --workpath build packaging/clawd.spec
SIGN=${CLAWD_SIGN_IDENTITY:-}
if [ -n "$SIGN" ]; then
  # PyInstaller signed every binary and the bundle (clawd.spec); check it, do not redo it.
  codesign --verify --deep --strict --verbose=1 dist/Clawd.app
  echo "signed: $(codesign -dv --verbose=2 dist/Clawd.app 2>&1 | grep -E '^(Authority|TeamIdentifier)=' | head -2 | tr '\n' ' ')"
  if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    mkdir -p build
    ditto -c -k --keepParent dist/Clawd.app build/notarize.zip
    xcrun notarytool submit build/notarize.zip --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
      --password "$APPLE_APP_PASSWORD" --wait 2>&1 | tee build/notarize.log
    grep -q "status: Accepted" build/notarize.log || { echo "notarization was not accepted" >&2; exit 1; }
    xcrun stapler staple dist/Clawd.app
    spctl --assess --type execute --verbose=2 dist/Clawd.app
  else
    echo "signed but NOT notarized (APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD not all set)" >&2
  fi
else
  # Ad-hoc signature: unsigned apps from the internet still get the Gatekeeper
  # prompt once, but an ad-hoc signed bundle runs on Apple silicon at all.
  codesign --force --deep --sign - dist/Clawd.app
fi
VERSION=$(uv run -q --python "$PY" python -c 'import clawd_desktop as c; print(c.__version__)')
ARCH=$(uname -m)
STAGE=build/dmg
rm -rf "$STAGE"; mkdir -p "$STAGE"
cp -R dist/Clawd.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="dist/Clawd-$VERSION-mac-$ARCH.dmg"
rm -f "$DMG"
hdiutil create -quiet -volname "Clawd" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
echo "built $DMG"
# The self-update's file: the stapled app, zipped the way Finder does (keeps the signature).
ZIP="dist/Clawd-$VERSION-mac-$ARCH.zip"
rm -f "$ZIP"
ditto -c -k --keepParent dist/Clawd.app "$ZIP"
echo "built $ZIP"
