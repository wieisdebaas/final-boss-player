# Final Boss Player — User Manual

**Your NAS. Your Music. Always Playing.**

**Final Boss Player** (tech short name: **finalboss**) is a lightweight Omarchy music player. It indexes music folders you already have mounted (local disk or NAS), streams from those paths, and never copies your library onto the machine.

---

## First-time setup

### 1. Install the plugin + daemon

**From Omarchy (recommended for users):**

```sh
omarchy plugin add https://github.com/wieisdebaas/final-boss-player.git --enable
cd ~/.config/omarchy/plugins/io.github.finalboss.player
./scripts/install.sh
omarchy-restart-shell
```

`omarchy plugin add` installs the UI only. `scripts/install.sh` builds `finalbossd`
and enables the systemd user service. You need a Rust toolchain (`cargo`) once.

**From a git checkout (developers):**

```sh
./scripts/install.sh
omarchy plugin enable io.github.finalboss.player
omarchy-restart-shell
```

Add **Final Boss Player** to your bar if it is not already there (Omarchy bar / widget settings).

### 3. Add a music folder

1. Open the full player (right-click the bar widget, or open the panel).
2. Go to **Settings**.
3. Enter an absolute path to a mounted folder, for example:
   - `/mnt/nas/Music`
   - `/home/robin/Downloads/YT/Audio`
4. Click **Add Folder**. Final Boss scans in the background; progress shows in the player state.

Music stays on the share. Only a small SQLite index and artwork thumbnails are stored locally.

### 4. Check the daemon

```sh
systemctl --user status finalbossd
```

It should be **active (running)**. Logs:

```sh
journalctl --user -u finalbossd -f
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

The bar slot is the icon; hover it for the track name. The mini-player shows artwork, title, artist, progress, transport, shuffle, and volume. Click the speaker to mute, or drag the slider. Right-click the slider also mutes.

### Full player (panel)

Sidebar:

| Page | What it does |
| --- | --- |
| **Home** | Tracks added most recently |
| **Search** | Local search. Match chooses title, artist, album, or genre. The genre menu limits results to a parent family such as Hip-Hop. Playing a result plays that result list |
| **Library** | All tracks |
| **Artists** | Browse by artist |
| **Albums** | Browse by album |
| **Genres** | Browse by genre, as a list or as tiles |
| **Settings** | Folders, rescan, metadata lookup |

The bottom bar always shows the current track and play / pause / seek / volume. Click the **title**, or press `Ctrl+C`, to return to the list that queue came from — the album, the artist, the genre, or the library — scrolled to the playing song. That works from any page. The album name opens that album, the artist name opens that artist, and the genre name opens that genre. Closing the panel does **not** stop playback.

### Keyboard shortcuts (panel open)

The same list is on **Settings**, under **Keyboard**.

Playback works anywhere in the window:

| Key | Action |
| --- | --- |
| `Space` | Play / pause |
| `Ctrl+Left` or `Ctrl+P` | Previous track |
| `Ctrl+Right` or `Ctrl+N` | Next track |
| `Shift+Left` / `Shift+Right` | Seek −5s / +5s |
| `Ctrl+Up` / `Ctrl+Down` | Volume up / down |
| `Ctrl+M` | Mute |
| `Ctrl+S` | Toggle shuffle |
| `Ctrl+F` | Jump to Search |
| `Ctrl+L` | List view |
| `Ctrl+T` | Tiles view |
| `Ctrl+C` | Open the playing song’s list and center it |
| `Esc` | Close the panel |

One highlight sits on the sidebar or on the page list. `Tab` and `Shift+Tab` switch between them. With the highlight on Settings, those list keys scroll the page. In Search or the folder path, the arrow keys edit the text and `Ctrl+C` copies. Click outside the field to leave it. `Tab` leaves the field for the list, and `Shift+Tab` leaves it for the sidebar. `Enter` in the search field highlights the first result and does not play it; `Enter` on that result plays it. `Space` still pauses or resumes the song that is already playing. The highlight stays where the last key put it.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Previous / next row |
| `Left` / `Right` | Previous / next cover, in tile view |
| `Page Up` / `Page Down` | Jump one screen |
| `Ctrl+Home` / `Ctrl+End` | First / last item |
| `Enter` | Open the menu page, play the track, or open the album, artist, or genre |

**Shuffle** is the ⇄ control on the bottom player bar, or `Ctrl+S`. When on (accent colour), the rest of the queue is randomized so **Next** picks a shuffled order; the current song finishes first. Turn it off to return to album/list order from the current track.

**Library click:** tapping a track title starts it (or toggles pause if it is already loaded). Tapping the **artist** or **album** under the title opens that Artist or Album page. Lists have a **draggable scrollbar** on the right.

### Media keys

While `finalbossd` is running, system media keys and tools that speak MPRIS control Final Boss (player name `finalboss`). Playback continues with the panel closed.

---

## Managing your library

- **Add folder** — Settings → path → Add. Nested folders are scanned.
- **Remove folder** — Settings → remove that path. Tracks for that root leave the index; files on disk are untouched.
- **Rescan** — Settings → Rescan. Use a full rescan when a NAS has flaky timestamps or you renamed many files.
- **Same volume** — Settings → Same volume. Quiet and loud files are leveled so the next song plays at a similar volume. The slider still sets how loud the player is overall.
- **Update metadata while scanning** — Settings checkbox, off by default. When it is on, a scan fills in missing genre and album art from MusicBrainz. Files with no artist and album are identified with an AcoustID fingerprint. Nothing is written back into the audio files. The first pass over a large library is slow on purpose (about one album per second).
- **Offline share** — If a mount is missing, that folder shows as unavailable. Remount and rescan or wait for the next scan; the shell should stay responsive.

Supported for indexing: FLAC, MP3, M4A/AAC, WAV, OGG, OPUS.  
**Playback note:** Opus and ALAC-in-M4A are indexed but may not play with the current decoder.

---

## Where data lives

| Data | Location |
| --- | --- |
| Library index | `~/.local/state/finalboss/library.sqlite` |
| Last track / position | `~/.local/state/finalboss/playback.json` |
| Artwork thumbs | `~/.cache/finalboss/art/` |
| Daemon binary | `~/.local/lib/finalboss/finalbossd` |
| Plugin | `~/.config/omarchy/plugins/io.github.finalboss.player` |

Original audio files are never modified or copied into these locations.

---

## Troubleshooting

| Problem | What to try |
| --- | --- |
| No sound / empty library | `systemctl --user status finalbossd` — start or restart if needed |
| Plugin missing from bar | `omarchy plugin enable io.github.finalboss.player` then `omarchy-restart-shell` |
| Folder added but no tracks | Confirm the path is mounted and readable; watch scan in Settings / logs |
| NAS hangs or errors | Remount the share; check `journalctl --user -u finalbossd`; open/stat timeouts are 5s |
| Track won’t play | Check format (Opus/ALAC may fail); look for `file_unavailable` / `decode_failed` in state or logs |
| Media keys do nothing | Confirm daemon is running; another player may own the keys |

Uninstall:

```sh
./scripts/install.sh --uninstall
omarchy plugin remove io.github.finalboss.player --yes
omarchy-restart-shell
```

The library database and artwork cache are left in place unless you delete them yourself.

---

## What Final Boss Player does not do (v1)

No playlists, Spotify, cloud accounts, lyrics, or built-in SMB/NFS client. Online genre and cover lookup stays off unless **Update metadata while scanning** is checked. Use the OS to mount network shares, then point Final Boss Player at the mount path.
