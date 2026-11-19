#!/usr/bin/env bash
set -euo pipefail

BIN_PATH="${1:-target/release/screencast}"
VERSION="${2:-0.1.0}"
ARCH="amd64"
PKG_NAME="screencast_${VERSION}_${ARCH}"
STAGE_DIR="target/deb-stage/${PKG_NAME}"

if [ ! -x "$BIN_PATH" ]; then
  echo "Error: executable not found: $BIN_PATH" >&2
  exit 1
fi

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/DEBIAN" "$STAGE_DIR/usr/bin" "$STAGE_DIR/usr/share/doc/screencast" "$STAGE_DIR/usr/share/applications"

cp "$BIN_PATH" "$STAGE_DIR/usr/bin/screencast"
chmod 0755 "$STAGE_DIR/usr/bin/screencast"

cat > "$STAGE_DIR/DEBIAN/control" <<EOF
Package: screencast
Version: ${VERSION}
Section: video
Priority: optional
Architecture: ${ARCH}
Depends: ffmpeg
Recommends: pulseaudio-utils | pipewire-pulse, xdotool, gstreamer1.0-pipewire
Maintainer: screencast contributors <noreply@example.org>
Description: Mirror a Linux desktop to a TV live with low latency
 Cast desktop video and system audio over Google Cast, DLNA, Miracast,
 and web browsers (MSE) without installing any TV app.
EOF

cat > "$STAGE_DIR/usr/share/applications/screencast.desktop" <<'EOF'
[Desktop Entry]
Name=Screencast
GenericName=TV Screen Caster
Comment=Cast your Linux desktop live to a TV
Exec=screencast gui
Icon=video-display
Terminal=false
Type=Application
Categories=AudioVideo;Video;Network;
EOF

cp README.md "$STAGE_DIR/usr/share/doc/screencast/README.md"

mkdir -p dist
dpkg-deb --build --root-owner-group "$STAGE_DIR" "dist/${PKG_NAME}.deb"
echo "Package ready: dist/${PKG_NAME}.deb"
