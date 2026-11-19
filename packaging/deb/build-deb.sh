#!/usr/bin/env bash
set -euo pipefail

BIN_PATH="${1:-target/release/ekran}"
VERSION="${2:-0.1.0}"
ARCH="amd64"
PKG_NAME="ekran_${VERSION}_${ARCH}"
STAGE_DIR="target/deb-stage/${PKG_NAME}"

if [ ! -x "$BIN_PATH" ]; then
  echo "Hata: çalıştırılabilir dosya bulunamadı: $BIN_PATH" >&2
  exit 1
fi

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/DEBIAN" "$STAGE_DIR/usr/bin" "$STAGE_DIR/usr/share/doc/ekran" "$STAGE_DIR/usr/share/applications"

cp "$BIN_PATH" "$STAGE_DIR/usr/bin/ekran"
chmod 0755 "$STAGE_DIR/usr/bin/ekran"

cat > "$STAGE_DIR/DEBIAN/control" <<EOF
Package: ekran
Version: ${VERSION}
Section: video
Priority: optional
Architecture: ${ARCH}
Depends: ffmpeg
Recommends: pulseaudio-utils | pipewire-pulse, xdotool, gstreamer1.0-pipewire
Maintainer: ekran contributors <noreply@example.org>
Description: Linux masaustunu TV'ye canli ve dusuk gecikmeyle yansit
 TV'ye uygulama kurmadan Google Cast, DLNA ve web tarayicisi (MSE)
 uzerinden masaustu ve sistem sesi yansitma araci.
EOF

cat > "$STAGE_DIR/usr/share/applications/ekran.desktop" <<'EOF'
[Desktop Entry]
Name=Ekran
GenericName=TV Ekran Yansıtıcı
Comment=Linux masaüstünü TV'ye canlı yansıt
Exec=ekran gui
Icon=video-display
Terminal=false
Type=Application
Categories=AudioVideo;Video;Network;
EOF

cp README.md "$STAGE_DIR/usr/share/doc/ekran/README.md"

mkdir -p dist
dpkg-deb --build --root-owner-group "$STAGE_DIR" "dist/${PKG_NAME}.deb"
echo "Paket hazir: dist/${PKG_NAME}.deb"
