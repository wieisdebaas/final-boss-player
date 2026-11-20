# Endboss Music Player

Endboss is an Omarchy Quickshell music player plugin backed by the `endbossd`
Rust daemon. It streams from your music folders (including NAS mounts) with no
copying, keeps a SQLite index, and exposes media keys over MPRIS. The plugin id
is `io.github.endboss.player`.

**User guide:** [docs/MANUAL.md](docs/MANUAL.md)

## Install

```sh
scripts/install.sh            # build release, install, enable + start the unit, link plugin
scripts/install.sh --dry-run  # show what would happen
scripts/install.sh --uninstall
```

This installs:

- `~/.local/lib/endboss/endbossd` (release binary)
- `~/.config/systemd/user/endbossd.service` (rendered from `systemd/endbossd.service`)
- `~/.config/omarchy/plugins/io.github.endboss.player` (symlink to this repo)

It never restarts the Omarchy shell. Afterwards run `omarchy-restart-shell`
(or enable the plugin in shell settings) and add **Endboss Player** to the bar.
Then open the panel, go to **Settings**, and add your music folder.

```sh
systemctl --user status endbossd
journalctl --user -u endbossd -f
```

## Run from source

```sh
cd daemon
cargo run                  # start the daemon
cargo run -- --print-paths # print state, cache, runtime, db, art, and socket paths
scripts/dev-link.sh        # symlink the plugin only (--unlink to remove)
```

## Bar mini-player

| Input        | Action                         |
| ------------ | ------------------------------ |
| Left click   | Toggle mini-player popup       |
| Middle click | Play / pause                   |
| Right click  | Open the full player panel     |
| Scroll       | Previous / next track          |
| `Esc`        | Close the popup                |

The popup shows cover art, title, artist, progress, and previous / play / next.

The full player’s bottom bar also has **shuffle** (⇄). When enabled, upcoming tracks play in random order after the current song.

Clicking a track in Library / search / album lists starts it; clicking the **currently loaded** track toggles play / pause.

## Panel shortcuts

| Key                  | Action                |
| -------------------- | --------------------- |
| `Space`              | Play / pause          |
| `Ctrl+Left / Right`  | Previous / next       |
| `Shift+Left / Right` | Seek -5s / +5s        |
| `Ctrl+Up / Down`     | Volume up / down      |
| `M`                  | Mute                  |
| `Ctrl+F`             | Search                |
| `Esc`                | Close the panel       |

## Media keys (MPRIS)

While `endbossd` runs it registers `org.mpris.MediaPlayer2.endboss` on the
session bus, so keyboard media keys and `playerctl -p endboss ...` work with the
panel closed. Supported: Play, Pause, PlayPause, Stop (pauses), Next, Previous,
Seek, SetPosition, and PlaybackStatus / Metadata (title, artist, album, length,
cached art) / Position / Volume. Not supported: TrackList, Playlists, Raise/Quit,
writable Shuffle/LoopStatus/Volume, and the `Seeked` signal. If no session bus is
available the daemon logs a warning and keeps running without MPRIS.

## Known limitations

- **Network shares (NAS).** Scanning, opening a track for playback, and reading
  cover art all use time-limited I/O (5 s), so a dead mount reports
  `file_unavailable` / `player_busy` instead of freezing the daemon or the shell.
  A share that stalls *in the middle* of a stream is read inside the audio
  callback and cannot be time-limited: expect audible dropouts until it recovers.
- **Formats.** `.opus` files are indexed but cannot be played: the decoder
  (rodio/symphonia) is built without Opus support, so playback fails with
  `decode_failed`. ALAC in `.m4a` is not enabled either.
- **Logs.** `endbossd` logs to stderr (`journalctl --user -u endbossd`); set
  `RUST_LOG` (default `endbossd=info`) to change verbosity.

## Development

```sh
cd daemon
cargo test
cargo clippy --all-targets -- -D warnings
```
