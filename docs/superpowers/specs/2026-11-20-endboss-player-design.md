# ENDBOSS MUSIC PLAYER — Design

**Tagline:** Your NAS. Your Music. Always Playing.  
**Date:** 2026-11-20  
**Status:** Approved direction — simple Omarchy Quickshell player that streams from mounted folders

## Goal

A small, native Omarchy music player. Point it at mounted NAS (or local) folders, index metadata locally, and **stream playback from disk** — never copy the library onto the machine.

Inspired by OmaSpotify’s shell shape (service + bar mini-player + panel), not by Electron/Spotify online features.

## Non-goals (v1)

Playlists, Spotify, accounts, cloud, lyrics, recommendations, podcasts, online metadata, built-in SMB/NFS client.

## Architecture

```
BarWidget ←→ Service.qml ←→ Panel.qml
                 │
                 │ versioned JSON-RPC (Unix socket)
                 ▼
            endbossd (systemd --user)
         scan │ SQLite │ art thumbs │ player
                 │
                 ▼
         mounted NAS / local paths
```

| Piece | Role |
| --- | --- |
| **QML plugin** | UI, navigation, shortcuts, theme. No NAS walks. |
| **endbossd** | Source of truth: library index, scan, playback, errors. |
| **State** | `~/.local/state/endboss/` (DB, settings, last position) |
| **Cache** | `~/.cache/endboss/art/` (small thumbs only) |

Playback continues if the panel closes. On Quickshell restart, UI reconnects and loads a state snapshot from the daemon; audio does not stop.

## Streaming & folders

- User adds absolute paths to already-mounted shares (e.g. `/mnt/nas/Music`).
- Player opens files from those paths and streams; no full-file preload, no library copy.
- Missing/slow mounts: timed I/O, mark Unavailable, keep UI responsive.
- Manual rescan; incremental update using size/mtime when trustworthy; full rescan option when NAS timestamps lie.

## Library index (SQLite)

Store path, size, mtime, title/artist/album/album artist, track/disc/year/genre/duration, art presence, folder root id.  
Group by album artist + album; support Various Artists; preserve disc/track order.  
Never modify audio files. Never store full embedded images in the DB.

**Formats (v1):** FLAC, MP3, M4A/AAC, WAV, OGG, OPUS — best-effort where the decoder supports them; unsupported → clear error, don’t hang.

## Playback

Daemon player: play/pause, next/prev, seek, volume, shuffle, repeat, auto-advance, remember last position.  
Best-effort gapless in v1. Thin MPRIS so media keys work with the panel closed.

## UI (small)

- **Sidebar:** Home, Search, Library, Artists, Albums, Settings  
- **Home:** recently added / played, light stats  
- **Library / Artist / Album:** browse and play  
- **Search:** local only  
- **Bottom player + bar mini-player:** transport essentials  
- **Settings:** add/remove folders, rescan, art cache clear, shortcut list  
- **Theme:** follow Omarchy live theme; dark default feel  

Core shortcuts (OmaSpotify-like): Space, Ctrl+Left/Right, Shift+Left/Right seek, Ctrl+Up/Down volume, M mute, Ctrl+F search, Esc close mini/overlay.

## Packaging

Omarchy plugin (`service` + `bar-widget` + `panel`) plus `endbossd` binary and user unit. Develop in this repo; install/symlink into Omarchy plugins for use.

## Success criteria

- Add a mounted music folder → library appears → play streams from the NAS  
- Idle stays lean; no Electron; no polling loops  
- NAS blips don’t freeze the shell; playback survives panel close  

## Build order

1. Daemon skeleton + socket protocol + SQLite schema  
2. Scan + metadata  
3. Playback streaming  
4. Minimal QML shell (library + player + settings folders)  
5. Mini-player, search, home polish, MPRIS  
