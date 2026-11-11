#!/bin/zsh
# Build "Nether File Opener.app" (with Quick Look extension).
#   ./build.sh            build + sign into build/
#   ./build.sh install    also copy to /Applications and register Quick Look
set -euo pipefail
cd "${0:A:h}"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/app/Cargo.toml | head -1)
IDENTITY="${SIGN_IDENTITY:-Apple Development}"
TARGET=aarch64-apple-darwin
APP="build/Nether File Opener.app"
APPEX="$APP/Contents/PlugIns/NetherQuickLook.appex"

cargo build --release --target $TARGET -p nether-file-opener -p nether-quicklook
BIN=target/$TARGET/release

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APPEX/Contents/MacOS"
sed "s/__VERSION__/$VERSION/g" bundle/App-Info.plist > "$APP/Contents/Info.plist"
sed "s/__VERSION__/$VERSION/g" bundle/QL-Info.plist > "$APPEX/Contents/Info.plist"
cp $BIN/nether-file-opener "$APP/Contents/MacOS/NetherFileOpener"
cp $BIN/nether-quicklook "$APPEX/Contents/MacOS/NetherQuickLook"
cp assets/AppIcon.icns assets/FONT-LICENSE.txt "$APP/Contents/Resources/"

codesign --force --options runtime --timestamp=none -s "$IDENTITY" \
  --entitlements bundle/QL.entitlements "$APPEX"
codesign --force --options runtime --timestamp=none -s "$IDENTITY" "$APP"
codesign --verify --deep --strict "$APP"
echo "Built $APP"

if [[ "${1:-}" == "install" ]]; then
  DEST="/Applications/Nether File Opener.app"
  rm -rf "$DEST"
  cp -R "$APP" "$DEST"
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$DEST"
  pluginkit -a "$DEST/Contents/PlugIns/NetherQuickLook.appex"
  qlmanage -r >/dev/null 2>&1 || true
  qlmanage -r cache >/dev/null 2>&1 || true
  echo "Installed $DEST"
fi
