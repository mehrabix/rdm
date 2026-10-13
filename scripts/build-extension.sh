#!/usr/bin/env bash
#
# Build distributable browser-extension packages from `browser-extension/`:
#
#   rdm-extension-chromium.zip   unpacked tree (for "Load unpacked" / store upload)
#   rdm-extension-chromium.crx   signed CRX3 — needs a private key + google-chrome
#   rdm-extension-firefox.xpi    the same tree with the Firefox manifest
#   rdm-extension-firefox.zip    the same, for sideloading
#
# Usage: scripts/build-extension.sh <out-dir> [key.pem]
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/browser-extension"
OUT="${1:?usage: build-extension.sh <out-dir> [key.pem]}"
KEY="${2:-}"
mkdir -p "$OUT"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# --- Chromium: the tree as-is (its manifest.json is the Chromium one) ---------
(cd "$SRC" && zip -qr "$OUT/rdm-extension-chromium.zip" . -x '*.md' -x 'manifest.firefox.json')

# --- Firefox: swap in the Firefox manifest, ship as .xpi (and .zip) -----------
cp -r "$SRC/." "$work/firefox/"
rm -f "$work/firefox/manifest.json"
mv "$work/firefox/manifest.firefox.json" "$work/firefox/manifest.json"
(cd "$work/firefox" && zip -qr "$OUT/rdm-extension-firefox.xpi" . -x '*.md')
cp "$OUT/rdm-extension-firefox.xpi" "$OUT/rdm-extension-firefox.zip"

# --- Signed CRX3 (Chromium): Chrome's packer does the CRX3 signing -------------
if [ -n "$KEY" ] && [ -f "$KEY" ]; then
  if command -v google-chrome >/dev/null 2>&1; then
    cp "$KEY" "$work/key.pem"
    # Chrome writes `<dir>.crx` next to the directory it packs.
    google-chrome --no-sandbox --disable-gpu --pack-extension="$SRC" \
      --pack-extension-key="$work/key.pem" >/dev/null 2>&1 || true
    if [ -f "$ROOT/browser-extension.crx" ]; then
      mv "$ROOT/browser-extension.crx" "$OUT/rdm-extension-chromium.crx"
    else
      echo "warning: google-chrome did not produce a .crx" >&2
    fi
  else
    echo "warning: google-chrome not found; skipping the signed .crx" >&2
  fi
else
  echo "note: no key supplied; building unsigned packages only (.zip/.xpi)"
fi

echo "built into $OUT:"
ls -1 "$OUT"
