#!/usr/bin/env bash
# Build and install Final Boss Player's daemon for the current user.
#
#   scripts/install.sh              build release, install, enable + start unit
#   scripts/install.sh --dry-run    print what would happen, change nothing
#   scripts/install.sh --uninstall  stop/disable the unit, remove the binary
#
# Installs:
#   ~/.local/lib/finalboss/finalbossd                 release binary
#   ~/.config/systemd/user/finalbossd.service       user unit (from systemd/finalbossd.service)
#
# Plugin UI:
#   Prefer:  omarchy plugin add <repo> --enable
#   Then run this script from the installed plugin directory.
#   For local development from a git checkout, this script also tries
#   scripts/dev-link.sh (skipped if the plugin is already a real install).
#
# It never restarts the Omarchy shell; do that yourself afterwards
# (e.g. `omarchy-restart-shell`) or enable the plugin from shell settings.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIB_DIR="${FINALBOSS_LIB_DIR:-$HOME/.local/lib/finalboss}"
BINARY="$LIB_DIR/finalbossd"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT="$UNIT_DIR/finalbossd.service"
TEMPLATE="$REPO/systemd/finalbossd.service"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/daemon/target}"

DRY_RUN=0
UNINSTALL=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --uninstall) UNINSTALL=1 ;;
    -h|--help) sed -n '2,13p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

run() {
  if [[ $DRY_RUN -eq 1 ]]; then
    printf '[dry-run]'; printf ' %q' "$@"; printf '\n'
  else
    "$@"
  fi
}

have_systemd_user() {
  command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1
}

if [[ $UNINSTALL -eq 1 ]]; then
  if have_systemd_user || [[ $DRY_RUN -eq 1 ]]; then
    run systemctl --user disable --now finalbossd.service || true
  fi
  run rm -f "$UNIT" "$BINARY"
  run rmdir "$LIB_DIR" 2>/dev/null || true
  # Only unlink a development symlink; leave omarchy plugin add installs alone.
  if [[ -L "${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/plugins/io.github.finalboss.player" ]]; then
    run "$REPO/scripts/dev-link.sh" --unlink
  fi
  have_systemd_user && run systemctl --user daemon-reload || true
  echo "Uninstalled finalbossd. Library database and cache were left in place."
  echo "Remove the shell plugin with: omarchy plugin remove io.github.finalboss.player --yes"
  exit 0
fi

[[ -f "$TEMPLATE" ]] || { echo "missing $TEMPLATE" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "cargo not found; install a Rust toolchain" >&2; exit 1; }

echo "==> Building release binary"
run env CARGO_TARGET_DIR="$TARGET_DIR" cargo build --release --manifest-path "$REPO/daemon/Cargo.toml"

echo "==> Installing binary to $BINARY"
run mkdir -p "$LIB_DIR"
# Install via a temp name + rename so a running daemon's inode is not
# overwritten in place (avoids "text file busy").
run install -m 0755 "$TARGET_DIR/release/finalbossd" "$BINARY.new"
run mv -f "$BINARY.new" "$BINARY"

echo "==> Installing user unit to $UNIT"
run mkdir -p "$UNIT_DIR"
if [[ $DRY_RUN -eq 1 ]]; then
  echo "[dry-run] render $TEMPLATE with @BINARY@=$BINARY -> $UNIT"
else
  sed "s|@BINARY@|$BINARY|g" "$TEMPLATE" > "$UNIT"
fi

echo "==> Enabling finalbossd.service"
if have_systemd_user || [[ $DRY_RUN -eq 1 ]]; then
  run systemctl --user daemon-reload
  run systemctl --user enable finalbossd.service
  # restart so an upgrade picks up the new binary; start if it was stopped
  run systemctl --user restart finalbossd.service
else
  echo "systemd user session not available; skipping enable/start." >&2
  echo "Start manually: $BINARY" >&2
fi

PLUGIN_ID="io.github.finalboss.player"
PLUGIN_TARGET="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/plugins/$PLUGIN_ID"
if [[ -L "$PLUGIN_TARGET" ]] || [[ ! -e "$PLUGIN_TARGET" ]]; then
  echo "==> Linking Omarchy plugin (development)"
  run "$REPO/scripts/dev-link.sh"
elif [[ "$(cd "$PLUGIN_TARGET" && pwd -P)" == "$(cd "$REPO" && pwd -P)" ]]; then
  echo "==> Omarchy plugin already installed at $PLUGIN_TARGET (skipping link)"
else
  echo "==> Omarchy plugin already present at $PLUGIN_TARGET; not overwriting."
  echo "    Daemon install is complete. Enable with: omarchy plugin enable $PLUGIN_ID"
fi

cat <<MSG

Done. Next:
  1. Restart the Omarchy shell (omarchy-restart-shell) and enable/place the
     "Final Boss Player" bar widget.
  2. Open the panel, go to Settings and add your music folder (e.g. a NAS mount).
  3. Check the daemon:  systemctl --user status finalbossd
     Media keys / playerctl work via MPRIS (org.mpris.MediaPlayer2.finalboss).
MSG
