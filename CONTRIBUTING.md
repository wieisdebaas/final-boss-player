# Contributing

Thanks for helping with Final Boss Player.

## Setup

1. Clone the repo and use a Rust toolchain that can build `daemon/`.
2. Link into Omarchy for live UI work:

   ```sh
   ./scripts/dev-link.sh
   ./scripts/install.sh
   omarchy-restart-shell
   ```

3. Daemon work:

   ```sh
   cd daemon
   cargo test
   cargo clippy --all-targets -- -D warnings
   ```

## Layout

| Path | Role |
| --- | --- |
| `daemon/` | Rust playback daemon (`finalbossd`) |
| `qml/`, `*.qml` | Quickshell plugin UI |
| `manifest.json` | Omarchy plugin metadata |
| `scripts/` | Install / unlink helpers |
| `docs/MANUAL.md` | End-user docs |
| `docs/MARKETPLACE.md` | Marketplace tags and listing copy |

## Rules of the road

- Keep the daemon as the source of truth for playback and library state.
- Prefer timed I/O for anything that can touch a NAS path.
- Do not put symlinks inside the plugin directory (Omarchy `validate` rejects them).
- Match existing QML / Rust style; keep the UI Omarchy-theme friendly.
- Update `docs/MANUAL.md` when behaviour users see changes.

## Pull requests

- One focused change per PR when possible.
- Include a short test plan (daemon commands, UI steps, or both).
- Run `omarchy plugin validate .` before opening the PR.
