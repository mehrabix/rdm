#!/usr/bin/env bash
#
# Builds RDM's app icon from a master image and installs it everywhere on Linux:
# the window icon (embedded by the app), the taskbar/dock, and the app grid.
#
# The master artwork lives at `assets/icon-source.png`. Replace that file with
# new artwork (square, ideally 1024x1024) and re-run this script.
#
# Usage:  bash scripts/make-icon.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ASSETS="$ROOT/assets"
MASTER="$ASSETS/icon-source.png"

if [ ! -s "$MASTER" ]; then
  echo "missing master artwork: $MASTER" >&2
  echo "put a square PNG there (ideally 1024x1024) and re-run." >&2
  exit 1
fi

# 1. knock out the black backing that sits *behind* the rounded (white) plate —
#    otherwise the corners render as black instead of transparent — then
#    normalise to a square 1024x1024.
read -r W H <<<"$(magick identify -format '%w %h' "$MASTER")"
magick "$MASTER" -alpha set -fuzz 25% -fill none \
  -draw "alpha 1,1 floodfill" \
  -draw "alpha $((W - 2)),1 floodfill" \
  -draw "alpha 1,$((H - 2)) floodfill" \
  -draw "alpha $((W - 2)),$((H - 2)) floodfill" \
  -background none -resize 1024x1024^ -gravity center -extent 1024x1024 \
  "$ASSETS/icon.png"

# 2. derived sizes used by the app itself (icon-256 in the window, icon-32 tray)
for s in 32 64 128 256 512; do
  magick "$ASSETS/icon.png" -resize "${s}x${s}" "$ASSETS/icon-${s}.png"
done
magick "$ASSETS/icon.png" -define icon:auto-resize=256,128,64,48,32,16 "$ASSETS/icon.ico"

# 3. install into the user's icon theme so the shell picks it up
ICON_ROOT="$HOME/.local/share/icons/hicolor"
for s in 16 24 32 48 64 128 256 512; do
  dir="$ICON_ROOT/${s}x${s}/apps"
  mkdir -p "$dir"
  magick "$ASSETS/icon.png" -resize "${s}x${s}" "$dir/rdm.png"
done

# 4. desktop entry (GNOME/Wayland matches the icon through this, not the toolkit)
APP_DIR="$HOME/.local/share/applications"
mkdir -p "$APP_DIR"
EXEC_PATH="${RDM_EXEC:-$ROOT/target/dx/rdm/debug/linux/app/rdm}"
cat > "$APP_DIR/rdm.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=RDM
GenericName=Download Manager
Comment=Rust Download Manager — fast, resumable, multi-connection downloads
Exec=$EXEC_PATH
Icon=rdm
Terminal=false
Categories=Network;FileTransfer;
StartupWMClass=rdm
Keywords=download;http;https;ftp;manager;resume;
EOF

if command -v gtk-update-icon-cache >/dev/null; then
  gtk-update-icon-cache -f -t "$ICON_ROOT" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi

echo "icon built from $MASTER"
echo "  assets/icon.png (+ sizes, + icon.ico)"
echo "  installed under $ICON_ROOT"
echo "  installed $APP_DIR/rdm.desktop (Exec=$EXEC_PATH)"
