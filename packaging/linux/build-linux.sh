#!/usr/bin/env bash
# Build Linux release artifacts: .deb, AppImage and a portable tarball.
# Output goes to dist/. Requires: cargo-deb, curl (to fetch appimagetool once).
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(uname -m)"
APP=twm-bible-native
mkdir -p dist

cargo build --release --locked
cargo deb --no-build --output dist/
BIN="target/release/$APP"

# --- portable tarball ---
T="dist/$APP-$VERSION-linux-$ARCH"
rm -rf "$T"; mkdir -p "$T/bin" "$T/share/$APP" "$T/share/applications" "$T/share/icons/hicolor/256x256/apps"
cp "$BIN" "$T/bin/"
cp -r data "$T/share/$APP/data"
mkdir -p "$T/share/$APP/assets"; cp assets/twmintro.mp4 "$T/share/$APP/assets/"
cp packaging/linux/$APP.desktop "$T/share/applications/"
cp assets/icon-256.png "$T/share/icons/hicolor/256x256/apps/$APP.png"
cp README.md LICENSE fonts/OFL.txt "$T/"
cp packaging/linux/install.sh "$T/"
tar -C dist -czf "$T.tar.gz" "$(basename "$T")"
rm -rf "$T"

# --- AppImage ---
AD="target/AppDir"
rm -rf "$AD"; mkdir -p "$AD/usr/bin" "$AD/usr/share/$APP" "$AD/usr/share/applications" "$AD/usr/share/icons/hicolor/256x256/apps"
cp "$BIN" "$AD/usr/bin/"
cp -r data "$AD/usr/share/$APP/data"
mkdir -p "$AD/usr/share/$APP/assets"; cp assets/twmintro.mp4 "$AD/usr/share/$APP/assets/"
cp packaging/linux/$APP.desktop "$AD/usr/share/applications/"
cp packaging/linux/$APP.desktop "$AD/"
cp assets/icon-256.png "$AD/usr/share/icons/hicolor/256x256/apps/$APP.png"
cp assets/icon-256.png "$AD/$APP.png"
# Bundle libraries that winit/xkbcommon load with dlopen (not linked, so easy to miss).
mkdir -p "$AD/usr/lib"
for lib in libxkbcommon.so.0 libxkbcommon-x11.so.0; do
  p="$(/sbin/ldconfig -p | awk -v l="$lib" '$1==l && /x86-64|aarch64/ {print $NF; exit}')"
  if [[ -n "$p" ]]; then cp -L "$p" "$AD/usr/lib/"; else echo "warning: $lib not found on build host" >&2; fi
done
cat > "$AD/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export TWM_BIBLE_DATA="$HERE/usr/share/twm-bible-native/data"
export LD_LIBRARY_PATH="$HERE/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$HERE/usr/bin/twm-bible-native" "$@"
RUN
chmod +x "$AD/AppRun"
TOOL="target/appimagetool-$ARCH.AppImage"
if [[ ! -x "$TOOL" ]]; then
  curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
  chmod +x "$TOOL"
fi
# --appimage-extract-and-run avoids needing FUSE (containers, CI)
ARCH="$ARCH" "$TOOL" --appimage-extract-and-run --no-appstream "$AD" "dist/TWM_Bible-$VERSION-$ARCH.AppImage"
ls -lh dist
