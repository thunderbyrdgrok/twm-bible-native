#!/usr/bin/env bash
# Install TWM Bible from the unpacked tarball.
# Usage: ./install.sh [PREFIX]        (default PREFIX: ~/.local)
#        ./install.sh --uninstall [PREFIX]
set -euo pipefail
APP=twm-bible-native
HERE="$(cd "$(dirname "$0")" && pwd)"
UNINSTALL=0
if [[ "${1:-}" == "--uninstall" ]]; then UNINSTALL=1; shift; fi
PREFIX="${1:-$HOME/.local}"
if (( UNINSTALL )); then
  rm -f "$PREFIX/bin/$APP" "$PREFIX/share/applications/$APP.desktop" \
        "$PREFIX/share/icons/hicolor/256x256/apps/$APP.png"
  rm -rf "$PREFIX/share/$APP"
  echo "TWM Bible removed from $PREFIX (your notes in ~/.local/state/twm-bible-slint are kept)."
  exit 0
fi
install -Dm755 "$HERE/bin/$APP" "$PREFIX/bin/$APP"
mkdir -p "$PREFIX/share/$APP"
rm -rf "$PREFIX/share/$APP/data" "$PREFIX/share/$APP/assets"
cp -r "$HERE/share/$APP/data" "$HERE/share/$APP/assets" "$PREFIX/share/$APP/"
install -Dm644 "$HERE/share/icons/hicolor/256x256/apps/$APP.png" "$PREFIX/share/icons/hicolor/256x256/apps/$APP.png"
install -Dm644 "$HERE/share/applications/$APP.desktop" "$PREFIX/share/applications/$APP.desktop"
sed -i "s|^Exec=.*|Exec=$PREFIX/bin/$APP|" "$PREFIX/share/applications/$APP.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
echo "TWM Bible installed to $PREFIX. Open it from your app menu or run $PREFIX/bin/$APP"
