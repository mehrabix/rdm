#!/usr/bin/env bash
#
# Builds the RDM app icon and installs it everywhere on Linux.
#
# Idea: Ferris (the Rust crab) carrying your download — a white download arrow
# resting on the crab's shell, on a rust-orange squircle. Two elements only,
# so the mark stays readable down to 16px.
#
# Pieces:
#   crab   : Ferris (rustacean.net)
#   arrow  : Bootstrap Icons "download"
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ASSETS="$ROOT/assets"
SVG="$ASSETS/svg"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# --- 0. pieces (fetched once, then vendored) --------------------------------
mkdir -p "$SVG"
if [ ! -s "$SVG/ferris-happy.svg" ]; then
  curl -sSL -o "$SVG/ferris-happy.svg" \
    "https://www.rustacean.net/assets/rustacean-flat-happy.svg"
fi
if [ ! -s "$SVG/download-fill.svg" ]; then
  curl -sSL -o "$SVG/download-fill.svg" \
    "https://raw.githubusercontent.com/twbs/icons/main/icons/download.svg"
fi

# --- palette ----------------------------------------------------------------
RUST_HI='#FF9A4D'
RUST_LO='#D9541E'
CRAB='#241A19'      # crab body (near-black, warm)
CRAB_SHADE='#3B2A24'
INK='#FFFFFF'

# --- 1. squircle background -------------------------------------------------
magick -size 1024x1024 gradient:"$RUST_HI"-"$RUST_LO" "$TMP/grad.png"
magick -size 1024x1024 xc:none -fill white \
  -draw "roundrectangle 8,8 1016,1016 236,236" "$TMP/mask.png"
magick "$TMP/grad.png" "$TMP/mask.png" -alpha off \
  -compose CopyOpacity -composite "$TMP/base.png"

# --- 2. rasterise the pieces ------------------------------------------------
# recolour Ferris from rust-orange to near-black so it reads on the orange bg
sed -e "s/rgb(247,76,0)/$CRAB/g" -e "s/rgb(165,43,0)/$CRAB_SHADE/g" \
    "$SVG/ferris-happy.svg" > "$TMP/ferris-ink.svg"
magick -background none "$TMP/ferris-ink.svg" -resize 780x "$TMP/crab.png"

sed "s/currentColor/$INK/" "$SVG/download-fill.svg" > "$TMP/arrow.svg"
magick -background none "$TMP/arrow.svg" -resize 210x "$TMP/arrow.png"

# --- 3. compose: arrow resting on the crab's shell --------------------------
magick "$TMP/base.png" \
  "$TMP/crab.png" -gravity center -geometry +0+60 -composite \
  "$TMP/arrow.png" -gravity center -geometry +0+96 -composite \
  "$ASSETS/icon.png"

# --- 4. derived sizes -------------------------------------------------------
for s in 32 64 128 256 512; do
  magick "$ASSETS/icon.png" -resize "${s}x${s}" "$ASSETS/icon-${s}.png"
done
magick "$ASSETS/icon.png" -define icon:auto-resize=256,128,64,48,32,16 "$ASSETS/icon.ico"

# --- 5. install into the user's theme ---------------------------------------
ICON_ROOT="$HOME/.local/share/icons/hicolor"
for s in 16 24 32 48 64 128 256 512; do
  dir="$ICON_ROOT/${s}x${s}/apps"
  mkdir -p "$dir"
  magick "$ASSETS/icon.png" -resize "${s}x${s}" "$dir/rdm.png"
done

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

echo "icon written to $ASSETS/icon.png (+ sizes, + icon.ico)"
echo "installed icon theme entries under $ICON_ROOT"
echo "installed $APP_DIR/rdm.desktop (Exec=$EXEC_PATH)"
