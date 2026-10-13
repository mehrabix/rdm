#!/usr/bin/env bash
#
# Builds a Debian package from a built app directory.
#
#   packaging/linux/build-deb.sh <app-dir> <version> [out-dir] [host-bin]
#
# <app-dir> is the directory `dx build --release` produces (it contains the
# `rdm` binary). [host-bin] is the optional browser native-messaging host
# (`rdm-native-host`); when given it is installed, with its manifests, so the
# browser extension can reach the app. The package installs the app to
# /usr/lib/rdm with a /usr/bin/rdm wrapper, a desktop entry and the icon, so it
# integrates with the app grid.
#
set -euo pipefail

APP_DIR="${1:?usage: build-deb.sh <app-dir> <version> [out-dir] [host-bin]}"
VERSION="${2:?usage: build-deb.sh <app-dir> <version> [out-dir] [host-bin]}"
OUT_DIR="${3:-.}"
HOST_BIN="${4:-}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$(find "$APP_DIR" -type f -name rdm | head -1)"
if [ -z "$BIN" ]; then
  echo "no rdm binary under $APP_DIR" >&2
  exit 1
fi

ARCH="$(dpkg --print-architecture)"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# payload
install -Dm755 "$BIN" "$STAGE/usr/lib/rdm/rdm"
if [ -d "$APP_DIR/assets" ]; then
  cp -r "$APP_DIR/assets" "$STAGE/usr/lib/rdm/"
fi

# Browser native-messaging host and its registration manifests.
if [ -n "$HOST_BIN" ] && [ -x "$HOST_BIN" ]; then
  install -Dm755 "$HOST_BIN" "$STAGE/usr/lib/rdm/rdm-native-host"
  render_host() { sed "s|@HOST@|/usr/lib/rdm/rdm-native-host|" "$ROOT/packaging/browser-host/$1"; }
  for dir in /etc/opt/chrome/native-messaging-hosts /etc/chromium/native-messaging-hosts; do
    install -d "$STAGE$dir"
    render_host io.github.mehrabix.rdm.chromium.json > "$STAGE$dir/io.github.mehrabix.rdm.json"
  done
  install -d "$STAGE/usr/lib/mozilla/native-messaging-hosts"
  render_host io.github.mehrabix.rdm.firefox.json > "$STAGE/usr/lib/mozilla/native-messaging-hosts/io.github.mehrabix.rdm.json"
fi

mkdir -p "$STAGE/usr/bin"
cat > "$STAGE/usr/bin/rdm" <<'WRAPPER'
#!/bin/sh
exec /usr/lib/rdm/rdm "$@"
WRAPPER
chmod 755 "$STAGE/usr/bin/rdm"

install -Dm644 "$ROOT/assets/icon-256.png" \
  "$STAGE/usr/share/icons/hicolor/256x256/apps/rdm.png"

install -d "$STAGE/usr/share/applications"
cat > "$STAGE/usr/share/applications/rdm.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=RDM
GenericName=Download Manager
Comment=Rust Download Manager — fast, resumable, multi-connection downloads
Exec=rdm
Icon=rdm
Terminal=false
Categories=Network;FileTransfer;
Keywords=download;http;https;ftp;manager;resume;
DESKTOP

# control metadata — Depends mirrors the runtime libraries Dioxus/WebKitGTK needs
mkdir -p "$STAGE/DEBIAN"
cat > "$STAGE/DEBIAN/control" <<CONTROL
Package: rdm
Version: $VERSION
Section: net
Priority: optional
Architecture: $ARCH
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libayatana-appindicator3-1, librsvg2-2
Maintainer: mehrabix <mehrabi@post.com>
Homepage: https://github.com/mehrabix/rdm
Description: Rust Download Manager
 A fast, resumable, multi-connection download manager for Linux, Windows and
 macOS. Splits a file into parallel range requests, resumes after interruption,
 and integrates with the desktop.
CONTROL

mkdir -p "$OUT_DIR"
DEB="$OUT_DIR/rdm_${VERSION}_${ARCH}.deb"
# The staging root maps to `/` on install, so it must not stay mktemp's 0700.
chmod 755 "$STAGE"
dpkg-deb --build --root-owner-group "$STAGE" "$DEB" >/dev/null
echo "$DEB"
