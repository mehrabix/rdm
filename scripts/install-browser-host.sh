#!/usr/bin/env bash
#
# Register the RDM native-messaging host with Chromium-based browsers and
# Firefox, for a per-user install.
#
#   scripts/install-browser-host.sh [path-to-rdm-native-host]
#
# The path defaults to `rdm-native-host` on PATH. A package install that puts
# the host at /usr/lib/rdm/rdm-native-host normally ships the manifests itself,
# so this script is for a manual/dev setup.
#
set -euo pipefail

HOST="${1:-}"
if [ -z "$HOST" ]; then
  for candidate in \
    /usr/lib/rdm/rdm-native-host \
    "$(command -v rdm-native-host 2>/dev/null || true)" \
    "$HOME/.cargo/bin/rdm-native-host"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
      HOST="$candidate"
      break
    fi
  done
fi
if [ -z "${HOST:-}" ] || [ ! -x "$HOST" ]; then
  echo "usage: install-browser-host.sh [path-to-rdm-native-host]" >&2
  echo "  install the 'rdm' package, or build it: cargo build --release -p rdm-native-host" >&2
  exit 1
fi
HOST="$(readlink -f "$HOST")"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHROMIUM_TEMPLATE="$ROOT/packaging/browser-host/io.github.mehrabix.rdm.chromium.json"
FIREFOX_TEMPLATE="$ROOT/packaging/browser-host/io.github.mehrabix.rdm.firefox.json"

render() {
  sed "s|@HOST@|$HOST|" "$1"
}

# Chromium / Chrome and common forks, per user.
for dir in \
  "$HOME/.config/google-chrome/NativeMessagingHosts" \
  "$HOME/.config/chromium/NativeMessagingHosts" \
  "$HOME/.config/microsoft-edge/NativeMessagingHosts" \
  "$HOME/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts"; do
  mkdir -p "$dir"
  render "$CHROMIUM_TEMPLATE" > "$dir/io.github.mehrabix.rdm.json"
  echo "installed $dir/io.github.mehrabix.rdm.json"
done

# Firefox, per user.
mkdir -p "$HOME/.mozilla/native-messaging-hosts"
render "$FIREFOX_TEMPLATE" > "$HOME/.mozilla/native-messaging-hosts/io.github.mehrabix.rdm.json"
echo "installed $HOME/.mozilla/native-messaging-hosts/io.github.mehrabix.rdm.json"

echo "done — native host: $HOST"
