### Repository URL

https://github.com/wieisdebaas/finalboss-player

### Category

Desktop

### Tags

media, quickshell, bar

### Suggest a missing tag

music

### Maintainer notes

Final Boss Player is an Omarchy Quickshell plugin (service + bar widget + panel) with a Rust playback daemon (`finalbossd`). It indexes and streams local/NAS folders (SMB/NFS mounts); no cloud accounts.

**Install (two steps):**
1. `omarchy plugin add https://github.com/wieisdebaas/finalboss-player.git --enable`
2. `cd ~/.config/omarchy/plugins/io.github.finalboss.player && ./scripts/install.sh` then `omarchy-restart-shell`

**Dependencies:** Rust/`cargo` (to build the daemon once), PipeWire/Pulse for audio. Optional: `playerctl` for CLI MPRIS control.

**Remove:** `./scripts/install.sh --uninstall` then `omarchy plugin remove io.github.finalboss.player --yes`.

### Submission checklist

- [x] The repository is public and contains installation and removal instructions.
- [x] I have documented the plugin license and any external dependencies.
- [x] I confirm that I own or have permission to submit this plugin and its preview assets.
- [x] The plugin does not overwrite user configuration without explicit consent.
- [x] I understand that approval is for listing and is not a security review.
