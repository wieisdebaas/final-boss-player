#!/usr/bin/env bash
# Symlink this repo into Omarchy's plugin directory for development.
#
#   scripts/dev-link.sh            create the symlink
#   scripts/dev-link.sh --unlink   remove it
#
# This never restarts the shell. Afterwards either restart it yourself
# (e.g. `omarchy-restart-shell`) or enable the plugin from Omarchy's shell
# settings. finalbossd must be running for the UI to connect:
#   cargo run --manifest-path daemon/Cargo.toml
set -euo pipefail

PLUGIN_ID="io.github.finalboss.player"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGINS_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/plugins"
TARGET="$PLUGINS_DIR/$PLUGIN_ID"

if [[ "${1:-}" == "--unlink" ]]; then
  if [[ -L "$TARGET" ]]; then
    rm "$TARGET"
    echo "Removed $TARGET"
  else
    echo "Nothing to remove at $TARGET"
  fi
  exit 0
fi

[[ -f "$REPO/manifest.json" ]] || { echo "manifest.json not found in $REPO" >&2; exit 1; }

mkdir -p "$PLUGINS_DIR"

if [[ -L "$TARGET" ]]; then
  ln -sfn "$REPO" "$TARGET"
elif [[ -e "$TARGET" ]]; then
  echo "$TARGET exists and is not a symlink; refusing to overwrite" >&2
  exit 1
else
  ln -s "$REPO" "$TARGET"
fi

echo "Linked $TARGET -> $REPO"
echo "Next: restart the Omarchy shell (or enable the plugin), start finalbossd,"
echo "then open the panel: omarchy-shell shell summon $PLUGIN_ID '{}'"
