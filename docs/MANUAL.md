# Endboss Player — User Manual

**Your NAS. Your Music. Always Playing.**

Endboss Player is a lightweight Omarchy music player. It indexes music folders you already have mounted (local disk or NAS), streams from those paths, and never copies your library onto the machine.

---

## First-time setup

### 1. Install

From the project directory:

```sh
scripts/install.sh
```

That installs the `endbossd` daemon, a systemd user service, and links the Omarchy plugin.

### 2. Enable the UI

```sh
omarchy plugin enable io.github.endboss.player
omarchy-restart-shell
```

Add **Endboss Player** to your bar if it is not already there (Omarchy bar / widget settings).

### 3. Add a music folder

1. Open the full player (right-click the bar widget, or open the panel).
2. Go to **Settings**.
3. Enter an absolute path to a mounted folder, for example:
   - `/mnt/nas/Music`
   - `/home/robin/Downloads/YT/Audio`
4. Click **Add Folder**. Endboss scans in the background; progress shows in the player state.

Music stays on the share. Only a small SQLite index and artwork thumbnails are stored locally.

### 4. Check the daemon

```sh
systemctl --user status endbossd
```

It should be **active (running)**. Logs:

```sh
journalctl --user -u endbossd -f
```

---

## Everyday use

### Bar widget (mini-player)

| Input | Action |
| --- | --- |
| Left click | Open / close mini-player |
| Middle click | Play / pause |
| Right click | Open full player |
| Scroll | Previous / next track |
| `Esc` | Close mini-player |

The mini-player shows artwork, title, artist, progress, and transport controls.

### Full player (panel)

Sidebar:

| Page | What it does |
| --- | --- |
| **Home** | Library overview |
| **Search** | Local search (no internet) |
| **Library** | All tracks |
| **Artists** | Browse by artist |
| **Albums** | Browse by album |
| **Settings** | Folders, rescan |

The bottom bar always shows the current track and play / pause / seek / volume. Closing the panel does **not** stop playback.

### Keyboard shortcuts (panel open)

| Key | Action |
| --- | --- |
| `Space` | Play / pause |
| `Ctrl+Left` / `Ctrl+Right` | Previous / next |
| `Shift+Left` / `Shift+Right` | Seek −5s / +5s |
| `Ctrl+Up` / `Ctrl+Down` | Volume up / down |
| `M` | Mute |
| `Ctrl+F` | Jump to Search |
| `Esc` | Close the panel |

**Shuffle** is the ⇄ control on the bottom player bar. When on (accent colour), the rest of the queue is randomized so **Next** picks a shuffled order; the current song finishes first. Turn it off to return to album/list order from the current track.

**Library click:** tapping a different track starts it; tapping the track that is already loaded toggles play / pause.

### Media keys

While `endbossd` is running, system media keys and tools that speak MPRIS control Endboss (player name `endboss`). Playback continues with the panel closed.

---

## Managing your library

- **Add folder** — Settings → path → Add. Nested folders are scanned.
- **Remove folder** — Settings → remove that path. Tracks for that root leave the index; files on disk are untouched.
- **Rescan** — Settings → Rescan. Use a full rescan when a NAS has flaky timestamps or you renamed many files.
- **Offline share** — If a mount is missing, that folder shows as unavailable. Remount and rescan or wait for the next scan; the shell should stay responsive.

Supported for indexing: FLAC, MP3, M4A/AAC, WAV, OGG, OPUS.  
**Playback note:** Opus and ALAC-in-M4A are indexed but may not play with the current decoder.

---

## Where data lives

| Data | Location |
| --- | --- |
| Library index | `~/.local/state/endboss/library.sqlite` |
| Last track / position | `~/.local/state/endboss/playback.json` |
| Artwork thumbs | `~/.cache/endboss/art/` |
| Daemon binary | `~/.local/lib/endboss/endbossd` |
| Plugin | `~/.config/omarchy/plugins/io.github.endboss.player` |

Original audio files are never modified or copied into these locations.

---

## Troubleshooting

| Problem | What to try |
| --- | --- |
| No sound / empty library | `systemctl --user status endbossd` — start or restart if needed |
| Plugin missing from bar | `omarchy plugin enable io.github.endboss.player` then `omarchy-restart-shell` |
| Folder added but no tracks | Confirm the path is mounted and readable; watch scan in Settings / logs |
| NAS hangs or errors | Remount the share; check `journalctl --user -u endbossd`; open/stat timeouts are 5s |
| Track won’t play | Check format (Opus/ALAC may fail); look for `file_unavailable` / `decode_failed` in state or logs |
| Media keys do nothing | Confirm daemon is running; another player may own the keys |

Uninstall:

```sh
scripts/install.sh --uninstall
```

The library database and artwork cache are left in place unless you delete them yourself.

---

## What Endboss does not do (v1)

No playlists, Spotify, cloud accounts, lyrics, online metadata, or built-in SMB/NFS client. Use the OS to mount network shares, then point Endboss at the mount path.
