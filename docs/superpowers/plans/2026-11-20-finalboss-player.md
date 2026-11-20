# ENDBOSS MUSIC PLAYER Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a small Omarchy Quickshell plugin that indexes mounted music folders in SQLite and streams playback from disk via a Rust daemon.

**Architecture:** Omarchy plugin (`Service` / `BarWidget` / `Panel`) talks JSON-RPC over a Unix socket to `finalbossd`. The daemon owns scan, SQLite, artwork thumbs, and streaming playback. Music files never leave the NAS mount.

**Tech Stack:** Quickshell QML, Rust (tokio, rusqlite, lofty, symphonia, cpal/rodio or similar), SQLite, systemd user unit, Omarchy plugin manifest.

**Spec:** `docs/superpowers/specs/2026-11-20-finalboss-player-design.md`

## Global Constraints

- No Electron / Chromium / heavy web UI
- Never copy the music library; stream from mounted paths only
- Never modify audio files; no online metadata
- Daemon is source of truth for playback state
- Timed NAS I/O; Unavailable must not freeze Quickshell
- Lazy artwork thumbs only under `~/.cache/finalboss/art/`
- State/DB under `~/.local/state/finalboss/`
- Plugin id: `io.github.finalboss.player`
- Socket: `$XDG_RUNTIME_DIR/finalboss/backend.sock`
- Protocol: UTF-8 JSON, one object per line, `"v":1`
- No playlists / Spotify / lyrics / accounts in v1

## File Structure

```
finalboss-player/
├── README.md
├── manifest.json                 # Omarchy plugin
├── Service.qml                   # shared UI state + IPC
├── BackendClient.qml             # socket client
├── BarWidget.qml                 # bar icon + mini-player
├── Panel.qml                     # full window shell
├── qml/
│   ├── HomePage.qml
│   ├── LibraryPage.qml
│   ├── SearchPage.qml
│   ├── AlbumPage.qml
│   ├── ArtistPage.qml
│   ├── SettingsPage.qml
│   ├── PlayerBar.qml
│   └── components/…              # rows, buttons, art
├── systemd/finalbossd.service
├── scripts/install.sh
├── daemon/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   ├── paths.rs              # XDG state/cache/runtime
│   │   ├── protocol.rs           # request/response/event types
│   │   ├── server.rs             # accept clients, dispatch
│   │   ├── state.rs              # snapshot broadcast
│   │   ├── db.rs                 # schema + queries
│   │   ├── scan.rs               # async folder scan
│   │   ├── meta.rs               # lofty tags
│   │   ├── art.rs                # thumb extract/cache
│   │   ├── player.rs             # stream decode + output
│   │   └── mpris.rs              # thin MPRIS
│   └── tests/
└── docs/…
```

---

### Task 1: Daemon crate + XDG paths

**Files:**
- Create: `daemon/Cargo.toml`
- Create: `daemon/src/main.rs`
- Create: `daemon/src/paths.rs`
- Create: `daemon/tests/paths_test.rs`
- Create: `README.md` (minimal run instructions)

**Interfaces:**
- Produces: `paths::Paths { state_dir, cache_dir, runtime_dir, db_path, art_dir, socket_path }` via `Paths::from_env()` using `XDG_STATE_HOME`, `XDG_CACHE_HOME`, `XDG_RUNTIME_DIR` with home fallbacks; creates dirs on `ensure()`.

- [ ] **Step 1: Scaffold crate**

`daemon/Cargo.toml`:

```toml
[package]
name = "finalbossd"
version = "0.1.0"
edition = "2021"
license = "MIT"

[[bin]]
name = "finalbossd"
path = "src/main.rs"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
directories = "5"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["io-util", "macros", "net", "rt-multi-thread", "signal", "sync", "time", "fs"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Implement `paths.rs` + failing test for socket path under runtime dir**

```rust
// tests assert Paths::from_env_with(home, state, cache, runtime)
// socket_path == runtime.join("finalboss/backend.sock")
// db_path == state.join("finalboss/library.sqlite")
```

- [ ] **Step 3: `main.rs` prints paths with `--print-paths` and exits 0**

- [ ] **Step 4: Run tests**

Run: `cd daemon && cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add daemon README.md
git commit -m "Add finalbossd crate with XDG paths"
```

---

### Task 2: Protocol types (v1)

**Files:**
- Create: `daemon/src/protocol.rs`
- Create: `daemon/tests/protocol_test.rs`
- Modify: `daemon/src/main.rs` (mod protocol)

**Interfaces:**
- Produces serde types:
  - `ClientMessage { v: u32, id: u64, #[serde(flatten)] cmd: Command }`
  - `Command` enum: `Hello`, `Ping`, `GetState`, `AddFolder { path }`, `RemoveFolder { path }`, `Rescan { full: bool }`, `Query { kind, q, limit, offset }`, `Play { track_id }`, `PlayAlbum { album_id }`, `Pause`, `Toggle`, `Next`, `Previous`, `Seek { position_ms }`, `SetVolume { level: f32 }`, `SetShuffle { enabled }`, `SetRepeat { mode }`, `GetArt { album_id }`
  - `ServerMessage`: `Response { id, ok, result?, error? }` | `Event { event, state? }`
  - `PlayerState` snapshot fields: `lifecycle`, `playing`, `track`, `position_ms`, `duration_ms`, `volume`, `shuffle`, `repeat`, `scan`, `folders`, `error_code`, `error_message`, `generation`
- Reject `v != 1` with error code `unsupported_version`

- [ ] **Step 1: Write round-trip tests for `pause` and `seek` JSON lines**

- [ ] **Step 2: Implement `protocol.rs` until tests pass**

- [ ] **Step 3: Commit**

```bash
git add daemon/src/protocol.rs daemon/tests/protocol_test.rs
git commit -m "Define finalbossd JSON-RPC protocol v1"
```

---

### Task 3: SQLite schema + library queries

**Files:**
- Create: `daemon/src/db.rs`
- Create: `daemon/tests/db_test.rs`
- Modify: `daemon/Cargo.toml` (add `rusqlite` with `bundled`)

**Interfaces:**
- Produces: `Db::open(path) -> Db`
- Tables: `folders(id, path UNIQUE, last_scan_at)`, `tracks(...)`, `artists`, `albums`, `plays(track_id, played_at)`
- Methods: `upsert_folder`, `list_folders`, `upsert_track`, `remove_missing_under`, `search`, `list_albums`, `list_artists`, `album_tracks`, `stats`, `record_play`

Schema essentials for `tracks`: `id INTEGER PK`, `folder_id`, `path UNIQUE`, `size`, `mtime`, `title`, `artist`, `album_artist`, `album`, `track_no`, `disc_no`, `year`, `genre`, `duration_ms`, `has_art`, `added_at`.

- [ ] **Step 1: Test insert track + search by title on tempfile DB**

- [ ] **Step 2: Implement schema migration `PRAGMA user_version = 1` and queries**

- [ ] **Step 3: `cargo test` PASS → commit**

```bash
git commit -m "Add SQLite library schema and queries"
```

---

### Task 4: Socket server + state broadcast

**Files:**
- Create: `daemon/src/state.rs`
- Create: `daemon/src/server.rs`
- Modify: `daemon/src/main.rs` (run server loop)

**Interfaces:**
- `AppState` behind `tokio::sync::RwLock` + `broadcast` for `PlayerState`
- On connect: send `event: state_changed` full snapshot
- Dispatch commands; unknown → `ok:false` `unknown_command`
- Bind socket after removing stale sock file; `umask` user-only

- [ ] **Step 1: Integration test: connect via UnixStream, send Ping, get ok response**

- [ ] **Step 2: Implement server accept loop + hello/ping/get_state**

- [ ] **Step 3: Commit**

```bash
git commit -m "Add finalbossd Unix socket server and state snapshots"
```

---

### Task 5: Folder scan + metadata

**Files:**
- Create: `daemon/src/meta.rs`
- Create: `daemon/src/scan.rs`
- Modify: `daemon/Cargo.toml` (add `lofty`, `walkdir`)
- Modify: `server` command handlers for AddFolder/RemoveFolder/Rescan
- Test: `daemon/tests/scan_test.rs` with fixture audio or synthetic tags if available; otherwise test walk + stub meta

**Interfaces:**
- `meta::read(path) -> TrackMeta` via lofty (title/artist/album/…/duration/has_art)
- `scan::run(db, folder, full, cancel)` walks extensions: `flac,mp3,m4a,aac,wav,ogg,opus`
- Stat timeout: skip/mark unavailable after 5s per file open/stat
- Incremental: skip unchanged `(size,mtime)` unless `full`
- Emits scan progress on state (`scanning`, `scanned`, `total`)

- [ ] **Step 1: Unit-test extension filter + unchanged skip logic**

- [ ] **Step 2: Implement meta + scan; wire AddFolder to spawn scan task**

- [ ] **Step 3: Manual smoke: `finalbossd` + JSON `add_folder` on a tiny local music dir**

- [ ] **Step 4: Commit**

```bash
git commit -m "Scan mounted folders and index metadata in SQLite"
```

---

### Task 6: Streaming playback engine

**Files:**
- Create: `daemon/src/player.rs`
- Modify: `daemon/Cargo.toml` (symphonia + rubato optional; `cpal` or `rodio`)
- Wire play/pause/next/prev/seek/volume/shuffle/repeat
- Persist last track id + position into state dir on pause/stop/interval

**Interfaces:**
- `Player::play_path(path, start_ms)` streams decode → audio device (no full read)
- Queue from album/search play commands
- On missing file: set `error_code=file_unavailable`, advance or stop
- Best-effort gapless: keep decoder warm for next same-format track when cheap; otherwise hard cut OK

- [ ] **Step 1: Test queue next/prev ordering without audio device (logical queue)**

- [ ] **Step 2: Implement streaming play for at least FLAC/MP3**

- [ ] **Step 3: Smoke-play one local file via socket `play`**

- [ ] **Step 4: Commit**

```bash
git commit -m "Stream local audio playback from finalbossd"
```

---

### Task 7: Artwork thumbs + query APIs

**Files:**
- Create: `daemon/src/art.rs`
- Extend `db`/`server` for `query` kinds: `tracks|albums|artists|search|home|album|artist`
- Lazy: extract embedded image on first `get_art`, write JPEG/PNG thumb ≤300px to cache, return path

- [ ] **Step 1: Test cache path key stable for album_id**

- [ ] **Step 2: Implement art extract + query handlers returning JSON lists**

- [ ] **Step 3: Commit**

```bash
git commit -m "Add lazy artwork cache and library query RPC"
```

---

### Task 8: Omarchy plugin shell (Service + BackendClient + Panel stub)

**Files:**
- Create: `manifest.json`
- Create: `Service.qml`
- Create: `BackendClient.qml`
- Create: `Panel.qml` (sidebar + stack + PlayerBar placeholder)
- Create: `qml/PlayerBar.qml`
- Create: `qml/SettingsPage.qml` (add folder path + rescan)
- Create: `qml/LibraryPage.qml` (list tracks from query)

**Interfaces:**
- `BackendClient` mirrors OmaSpotify pattern: line-delimited JSON, reconnect, `send(command)`
- `Service.qml` holds `playerState`, `navigate(page)`, shortcuts; only shared brain
- Panel shows Library + Settings first

- [ ] **Step 1: manifest with kinds service/bar-widget/panel, id `io.github.finalboss.player`**

- [ ] **Step 2: BackendClient connect + ping; Service exposes state**

- [ ] **Step 3: Settings add folder → daemon; Library lists tracks**

- [ ] **Step 4: Install/symlink plugin, restart shell, smoke UI**

- [ ] **Step 5: Commit**

```bash
git commit -m "Add Omarchy plugin shell wired to finalbossd"
```

---

### Task 9: Browse pages + search + home

**Files:**
- Create: `qml/HomePage.qml`, `SearchPage.qml`, `AlbumPage.qml`, `ArtistPage.qml`
- Flesh out `Panel.qml` sidebar navigation
- Keyboard shortcuts in Service/Panel

- [ ] **Step 1: Wire Home/Search/Album/Artist to `query` RPC**

- [ ] **Step 2: Play album / play track from UI**

- [ ] **Step 3: Shortcuts: Space, Ctrl+Left/Right, Shift seek, Ctrl+Up/Down, M, Ctrl+F, Esc**

- [ ] **Step 4: Commit**

```bash
git commit -m "Add library browse, search, home, and shortcuts"
```

---

### Task 10: Bar mini-player + systemd install + MPRIS

**Files:**
- Create: `BarWidget.qml`
- Create: `systemd/finalbossd.service`
- Create: `scripts/install.sh`
- Create: `daemon/src/mpris.rs`
- Update: `README.md`

**Interfaces:**
- Mini-player: art, title, artist, prev/play/next; Esc closes
- User unit starts `finalbossd`; install script builds release, installs binary to `~/.local/lib/finalboss/`, enables unit, symlinks plugin
- MPRIS play/pause/next/prev/seek enough for media keys

- [ ] **Step 1: BarWidget mini-player using Service state**

- [ ] **Step 2: systemd unit + install script**

- [ ] **Step 3: MPRIS thin adapter**

- [ ] **Step 4: End-to-end: install → add NAS path → play stream → mini-player**

- [ ] **Step 5: Commit**

```bash
git commit -m "Add mini-player, MPRIS, and install packaging"
```

---

## Plan self-review

| Spec item | Task |
| --- | --- |
| Stream from mounts, no copy | 5, 6 |
| SQLite index + incremental/full rescan | 3, 5 |
| Daemon source of truth + reconnect | 4, 8 |
| Timed/unavailable NAS | 5, 6 |
| Lazy art cache | 7 |
| UI pages + player + mini-player | 8–10 |
| Shortcuts | 9 |
| Omarchy plugin packaging | 8, 10 |
| MPRIS | 10 |
| No playlists/Spotify/online | Global Constraints |

No TBD placeholders. Protocol/command names are consistent across tasks.
