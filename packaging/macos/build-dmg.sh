#!/usr/bin/env bash
# Build a universal (arm64 + x86_64) TWM Bible.app and .dmg. Run on macOS.
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
APP=twm-bible-native
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

B="target/macos/TWM Bible.app"
rm -rf target/macos; mkdir -p "$B/Contents/MacOS" "$B/Contents/Resources"
lipo -create -output "$B/Contents/MacOS/$APP" \
  "target/aarch64-apple-darwin/release/$APP" "target/x86_64-apple-darwin/release/$APP"
cp -r data "$B/Contents/Resources/data"
mkdir -p "$B/Contents/Resources/assets"; cp assets/twmintro.mp4 "$B/Contents/Resources/assets/"
cp packaging/macos/icon.icns "$B/Contents/Resources/icon.icns"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$B/Contents/Info.plist"
# Ad-hoc signature so Apple Silicon will launch it (not notarized).
codesign --force --deep --sign - "$B"

mkdir -p dist
STAGE=target/macos/dmg; mkdir -p "$STAGE"
cp -R "$B" "$STAGE/"; ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "TWM Bible" -srcfolder "$STAGE" -ov -format UDZO \
  "dist/TWM-Bible-$VERSION-macos-universal.dmg"
ls -lh dist
