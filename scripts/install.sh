#!/usr/bin/env bash
# Build and install Endboss for the current user.
#
#   scripts/install.sh              build release, install, enable + start unit, link plugin
#   scripts/install.sh --dry-run    print what would happen, change nothing
#   scripts/install.sh --uninstall  stop/disable the unit, remove the binary and plugin link
#
# Installs:
#   ~/.local/lib/endboss/endbossd                 release binary
#   ~/.config/systemd/user/endbossd.service       user unit (from systemd/endbossd.service)
#   ~/.config/omarchy/plugins/<plugin-id>         symlink to this repo (scripts/dev-link.sh)
#
# It never restarts the Omarchy shell; do that yourself afterwards
# (e.g. `omarchy-restart-shell`) or enable the plugin from shell settings.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIB_DIR="${ENDBOSS_LIB_DIR:-$HOME/.local/lib/endboss}"
BINARY="$LIB_DIR/endbossd"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT="$UNIT_DIR/endbossd.service"
TEMPLATE="$REPO/systemd/endbossd.service"
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
    run systemctl --user disable --now endbossd.service || true
  fi
  run rm -f "$UNIT" "$BINARY"
  run rmdir "$LIB_DIR" 2>/dev/null || true
  run "$REPO/scripts/dev-link.sh" --unlink
  have_systemd_user && run systemctl --user daemon-reload || true
  echo "Uninstalled. Library database and cache were left in place."
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
run install -m 0755 "$TARGET_DIR/release/endbossd" "$BINARY.new"
run mv -f "$BINARY.new" "$BINARY"

echo "==> Installing user unit to $UNIT"
run mkdir -p "$UNIT_DIR"
if [[ $DRY_RUN -eq 1 ]]; then
  echo "[dry-run] render $TEMPLATE with @BINARY@=$BINARY -> $UNIT"
else
  sed "s|@BINARY@|$BINARY|g" "$TEMPLATE" > "$UNIT"
fi

echo "==> Enabling endbossd.service"
if have_systemd_user || [[ $DRY_RUN -eq 1 ]]; then
  run systemctl --user daemon-reload
  run systemctl --user enable endbossd.service
  # restart so an upgrade picks up the new binary; start if it was stopped
  run systemctl --user restart endbossd.service
else
  echo "systemd user session not available; skipping enable/start." >&2
  echo "Start manually: $BINARY" >&2
fi

echo "==> Linking Omarchy plugin"
run "$REPO/scripts/dev-link.sh"

cat <<MSG

Done. Next:
  1. Restart the Omarchy shell (omarchy-restart-shell) and enable/place the
     "Endboss Player" bar widget.
  2. Open the panel, go to Settings and add your music folder (e.g. a NAS mount).
  3. Check the daemon:  systemctl --user status endbossd
     Media keys / playerctl work via MPRIS (org.mpris.MediaPlayer2.endboss).
MSG
