#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="0.1.0-1"
ARCH="all"
PKG_DIR="$ROOT/build/quickmemo_${VERSION}_${ARCH}"
OUT="$ROOT/dist/quickmemo_${VERSION}_${ARCH}.deb"

rm -rf "$PKG_DIR" "$ROOT/dist"
mkdir -p \
  "$PKG_DIR/DEBIAN" \
  "$PKG_DIR/usr/bin" \
  "$PKG_DIR/usr/share/applications" \
  "$PKG_DIR/usr/share/icons/hicolor/scalable/apps" \
  "$PKG_DIR/usr/share/doc/quickmemo/examples" \
  "$ROOT/dist"

install -m 0755 "$ROOT/src/quickmemo" "$PKG_DIR/usr/bin/quickmemo"
install -m 0644 "$ROOT/quickmemo.desktop" "$PKG_DIR/usr/share/applications/quickmemo.desktop"
install -m 0644 "$ROOT/quickmemo.svg" "$PKG_DIR/usr/share/icons/hicolor/scalable/apps/quickmemo.svg"
install -m 0644 "$ROOT/data/memos.md" "$PKG_DIR/usr/share/doc/quickmemo/examples/memos.md"
install -m 0644 "$ROOT/README.md" "$PKG_DIR/usr/share/doc/quickmemo/README.md"
install -m 0644 "$ROOT/LICENSE" "$PKG_DIR/usr/share/doc/quickmemo/LICENSE"

cat > "$PKG_DIR/DEBIAN/control" <<CONTROL
Package: quickmemo
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: simple <simple@example.com>
Depends: python3, python3-gi, gir1.2-gtk-4.0, gir1.2-pango-1.0, wl-clipboard, wmctrl
Recommends: gnome-text-editor | gedit
Description: Markdown based memo snippet picker
 Quick Memo is a small GTK 4 memo snippet picker. It stores snippets in a
 Markdown file, supports category navigation, search highlighting, single-line
 copy, full-card copy, delete confirmation, autostart, and hidden autostart.
CONTROL

find "$PKG_DIR" -type d -exec chmod 0755 {} +
dpkg-deb --build --root-owner-group "$PKG_DIR" "$OUT"

echo "$OUT"
