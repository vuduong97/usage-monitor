#!/bin/sh
# Build dist/Clawd.app and dist/Clawd-<version>-mac-<arch>.dmg (run on macOS).
set -eu
cd "$(dirname "$0")/.."
PY=${PYTHON:-3.12}
uv sync -q --python "$PY" --group build
uv run -q --python "$PY" python packaging/make_icons.py
rm -rf build/Clawd dist/Clawd dist/Clawd.app
uv run -q --python "$PY" --group build pyinstaller --noconfirm --clean --log-level WARN \
  --distpath dist --workpath build packaging/clawd.spec
# Ad-hoc signature: unsigned apps from the internet still get the Gatekeeper
# prompt once, but an ad-hoc signed bundle runs on Apple silicon at all.
codesign --force --deep --sign - dist/Clawd.app
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
