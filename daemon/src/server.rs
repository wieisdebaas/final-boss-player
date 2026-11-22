use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

use crate::player::{self, Cmd, PlayError, Player, QueueItem, Restore};
use crate::protocol::{
    parse_client_message, Command, ProtocolError, ServerMessage, PROTOCOL_VERSION,
};
use crate::scan;
use crate::state::AppState;

static UMASK_LOCK: Mutex<()> = Mutex::new(());

pub fn bind(socket_path: &Path) -> Result<UnixListener> {
    match fs::symlink_metadata(socket_path) {
        Ok(metadata) if !metadata.file_type().is_socket() => {
            bail!(
                "refusing to replace non-socket path {}",
                socket_path.display()
            );
        }
        Ok(_) => match StdUnixStream::connect(socket_path) {
            Ok(_) => bail!("socket {} is already in use", socket_path.display()),
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
                fs::remove_file(socket_path).with_context(|| {
                    format!("could not remove stale socket {}", socket_path.display())
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "could not determine whether {} is stale",
                        socket_path.display()
                    )
                });
            }
        },
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("could not inspect {}", socket_path.display()));
        }
    }

    let listener = bind_with_user_umask(socket_path)?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not secure {}", socket_path.display()))?;
    Ok(listener)
}

fn bind_with_user_umask(socket_path: &Path) -> Result<UnixListener> {
    let _guard = UMASK_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("socket umask lock was poisoned"))?;
    // umask is process-wide, so serialize the narrow create-and-restore section.
    let previous_umask = unsafe { libc::umask(0o077) };
    let result = UnixListener::bind(socket_path);
    unsafe {
        libc::umask(previous_umask);
    }
    result.with_context(|| format!("could not bind {}", socket_path.display()))
}

pub async fn run(socket_path: &Path, state: AppState) -> Result<()> {
    serve(bind(socket_path)?, state).await
}

pub async fn serve(listener: UnixListener, state: AppState) -> Result<()> {
    serve_context(listener, ServerContext::new(state, None, None, None)).await
}

/// Serves with the real audio engine. Playback position is persisted next to
/// the database in the state directory.
pub async fn run_with_db(socket_path: &Path, db_path: PathBuf, state: AppState) -> Result<()> {
    let art_dir = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("art");
    run_with_paths(socket_path, db_path, art_dir, state).await
}

pub async fn run_with_paths(
    socket_path: &Path,
    db_path: PathBuf,
    art_dir: PathBuf,
    state: AppState,
) -> Result<()> {
    // Create and migrate the database once, before anything can race to do it.
    // A broken or too-new database is a startup failure, not a per-request one.
    let open_path = db_path.clone();
    let folders = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        let db = crate::db::Db::open(&open_path)
            .with_context(|| format!("could not open library database {}", open_path.display()))?;
        let collapsed =
            db.rewrite_collaborations(|title, artist| match crate::meta::credit_featured_artists(
                title.to_owned(),
                Some(artist.to_owned()),
            ) {
                (title, Some(artist)) => (title, artist),
                (title, None) => (title, String::new()),
            })?;
        if collapsed > 0 {
            tracing::info!(
                tracks = collapsed,
                "moved slash-separated guests onto featuring credits"
            );
        }
        Ok(db.list_folders()?.into_iter().map(|f| f.path).collect())
    })
    .await
    .context("database startup task failed")??;
    state.set_folders(folders).await;

    let state_dir = db_path.parent().map(Path::to_path_buf);
    let persist_path = state_dir.map(|dir| dir.join(PLAYBACK_FILE));
    let restore = persist_path
        .as_deref()
        .and_then(|path| load_restore(&db_path, path));
    let player = Player::spawn_audio(
        state.clone(),
        tokio::runtime::Handle::current(),
        persist_path,
        restore,
    );
    let listener = bind(socket_path)?;
    crate::mpris::spawn(state.clone(), player.clone(), Some(art_dir.clone()));
    serve_context(
        listener,
        ServerContext::new(state, Some(db_path), Some(art_dir), Some(player)),
    )
    .await
}

/// Serves with a caller-supplied player (used with a fake audio backend).
pub async fn serve_with_player(
    listener: UnixListener,
    state: AppState,
    db_path: PathBuf,
    player: Player,
) -> Result<()> {
    let art_dir = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("art");
    serve_context(
        listener,
        ServerContext::new(state, Some(db_path), Some(art_dir), Some(player)),
    )
    .await
}

pub const PLAYBACK_FILE: &str = "playback.json";

/// Rebuilds the last queue paused at the saved position.
/// The saved scope selects the album, the artist, or the whole library.
pub fn load_restore(db_path: &Path, playback_path: &Path) -> Option<Restore> {
    let saved = player::load_playback(playback_path)?;
    let db = crate::db::Db::open(db_path).ok()?;
    let (tracks, start) = db.queue_for_scope(saved.track_id, &saved.scope).ok()??;
    Some(Restore {
        items: tracks.iter().map(queue_item).collect(),
        start,
        position_ms: saved.position_ms,
        scope: saved.scope,
    })
}

fn queue_item(track: &crate::db::Track) -> QueueItem {
    QueueItem {
        id: track.id,
        path: PathBuf::from(&track.path),
        duration_ms: u64::try_from(track.duration_ms).unwrap_or(0),
        meta: json!({
            "id": track.id,
            "title": track.title,
            "artist": track.artist,
            "album_artist": track.album_artist,
            "album": track.album,
            "track_no": track.track_no,
            "disc_no": track.disc_no,
            "year": track.year,
            "genre": track.genre,
            "duration_ms": track.duration_ms,
            "has_art": track.has_art,
            "album_id": track.album_id,
            "artist_id": track.artist_id,
        }),
    }
}

#[derive(Clone)]
struct ServerContext {
    state: AppState,
    db_path: Option<PathBuf>,
    art_dir: Option<PathBuf>,
    player: Option<Player>,
}

impl ServerContext {
    fn new(
        state: AppState,
        db_path: Option<PathBuf>,
        art_dir: Option<PathBuf>,
        player: Option<Player>,
    ) -> Self {
        Self {
            state,
            db_path,
            art_dir,
            player,
        }
    }
}

async fn serve_context(listener: UnixListener, context: ServerContext) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await.context("socket accept failed")?;
        let client_context = context.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_client(stream, client_context).await {
                tracing::debug!(%error, "socket client disconnected");
            }
        });
    }
}

async fn handle_client(stream: UnixStream, context: ServerContext) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut changes = context.state.subscribe();

    write_message(
        &mut writer,
        &ServerMessage::Event {
            event: "state_changed".into(),
            state: Some(context.state.snapshot().await),
        },
    )
    .await?;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line.context("could not read client message")? else {
                    return Ok(());
                };
                let response = dispatch(&line, &context).await;
                write_message(&mut writer, &response).await?;
            }
            changed = changes.recv() => {
                match changed {
                    Ok(snapshot) => {
                        write_message(
                            &mut writer,
                            &ServerMessage::Event {
                                event: "state_changed".into(),
                                state: Some(snapshot),
                            },
                        ).await?;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let snapshot = recover_after_lag(&mut changes, &context.state).await;
                        write_message(
                            &mut writer,
                            &ServerMessage::Event {
                                event: "state_changed".into(),
                                state: Some(snapshot),
                            },
                        ).await?;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        }
    }
}

pub(crate) async fn recover_after_lag(
    changes: &mut broadcast::Receiver<crate::protocol::PlayerState>,
    state: &AppState,
) -> crate::protocol::PlayerState {
    while let Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) = changes.try_recv() {}
    state.snapshot().await
}

async fn dispatch(line: &str, context: &ServerContext) -> ServerMessage {
    let raw: Value = match serde_json::from_str(line) {
        Ok(raw) => raw,
        Err(error) => return error_response(0, "invalid_request", error.to_string()),
    };
    let id = raw.get("id").and_then(Value::as_u64).unwrap_or(0);
    let command = raw.get("cmd").and_then(Value::as_str);

    if !matches!(
        command,
        Some(
            "hello"
                | "ping"
                | "get_state"
                | "add_folder"
                | "remove_folder"
                | "rescan"
                | "query"
                | "get_art"
                | "play"
                | "play_album"
                | "pause"
                | "toggle"
                | "next"
                | "previous"
                | "seek"
                | "set_volume"
                | "set_shuffle"
                | "set_repeat"
        )
    ) {
        return error_response(id, "unknown_command", "command is not supported");
    }

    let request = match parse_client_message(line) {
        Ok(request) => request,
        Err(error) => return protocol_error_response(id, error),
    };

    match request.cmd {
        Command::Hello => ok_response(request.id, Some(json!({ "v": PROTOCOL_VERSION }))),
        Command::Ping => ok_response(request.id, None),
        Command::GetState => ok_response(
            request.id,
            Some(
                serde_json::to_value(context.state.snapshot().await)
                    .expect("state is serializable"),
            ),
        ),
        Command::AddFolder { path } => add_folder(request.id, context, path).await,
        Command::RemoveFolder { path } => remove_folder(request.id, context, path).await,
        Command::Rescan { full } => rescan(request.id, context, full).await,
        Command::Query {
            kind,
            q,
            limit,
            offset,
        } => query(request.id, context, kind, q, limit, offset).await,
        Command::GetArt { album_id } => get_art(request.id, context, album_id).await,
        Command::Play { track_id, scope } => play_track(request.id, context, track_id, scope).await,
        Command::PlayAlbum { album_id } => play_album(request.id, context, album_id).await,
        Command::Pause => playback(request.id, context, Cmd::Pause).await,
        Command::Toggle => playback(request.id, context, Cmd::Toggle).await,
        Command::Next => playback(request.id, context, Cmd::Next).await,
        Command::Previous => playback(request.id, context, Cmd::Previous).await,
        Command::Seek { position_ms } => {
            playback(request.id, context, Cmd::Seek(position_ms)).await
        }
        Command::SetVolume { level } => playback(request.id, context, Cmd::SetVolume(level)).await,
        Command::SetShuffle { enabled } => {
            playback(request.id, context, Cmd::SetShuffle(enabled)).await
        }
        Command::SetRepeat { mode } => playback(request.id, context, Cmd::SetRepeat(mode)).await,
    }
}

async fn query(
    id: u64,
    context: &ServerContext,
    kind: String,
    q: String,
    limit: u32,
    offset: u32,
) -> ServerMessage {
    let Some(db_path) = context.db_path.clone() else {
        return error_response(id, "unavailable", "library database is not configured");
    };
    let limit = limit.min(500);
    let result = tokio::task::spawn_blocking(move || -> Result<Value> {
        let db = crate::db::Db::open(db_path)?;
        match kind.as_str() {
            "tracks" => Ok(serde_json::to_value(db.list_tracks(limit, offset)?)?),
            "albums" => Ok(serde_json::to_value(db.list_albums(limit, offset)?)?),
            "artists" => Ok(serde_json::to_value(db.list_artists(limit, offset)?)?),
            "search" => Ok(serde_json::to_value(db.search(&q, limit, offset)?)?),
            "home" => Ok(json!({
                "stats": db.stats()?,
                "tracks": db.list_tracks(limit, offset)?,
                "albums": db.list_albums(limit, offset)?,
                "artists": db.list_artists(limit, offset)?,
            })),
            "album" => {
                let album_id = parse_query_id(&q, "album")?;
                let tracks = db.album_tracks(album_id)?;
                Ok(serde_json::to_value(page(tracks, limit, offset))?)
            }
            "artist" => {
                let artist_id = parse_query_id(&q, "artist")?;
                Ok(serde_json::to_value(
                    db.artist_tracks(artist_id, limit, offset)?,
                )?)
            }
            _ => bail!("unsupported query kind: {kind}"),
        }
    })
    .await;

    match result {
        Ok(Ok(value)) => ok_response(id, Some(value)),
        Ok(Err(error)) => error_response(id, "invalid_query", error.to_string()),
        Err(error) => error_response(id, "internal_error", error.to_string()),
    }
}

fn parse_query_id(q: &str, entity: &str) -> Result<i64> {
    q.parse()
        .with_context(|| format!("{entity} query requires a numeric id"))
}

fn page<T>(items: Vec<T>, limit: u32, offset: u32) -> Vec<T> {
    items
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect()
}

async fn get_art(id: u64, context: &ServerContext, album_id: i64) -> ServerMessage {
    let (Some(db_path), Some(art_dir)) = (context.db_path.clone(), context.art_dir.clone()) else {
        return error_response(id, "unavailable", "artwork cache is not configured");
    };
    let result = tokio::task::spawn_blocking(move || -> Result<Option<PathBuf>> {
        let db = crate::db::Db::open(db_path)?;
        let Some(source) = db.album_art_source(album_id)? else {
            return Ok(None);
        };
        Ok(Some(crate::art::get_or_create(
            &art_dir,
            album_id,
            Path::new(&source),
        )?))
    })
    .await;

    match result {
        Ok(Ok(Some(path))) => ok_response(id, Some(json!({ "path": path }))),
        Ok(Ok(None)) => error_response(id, "not_found", "album has no embedded artwork"),
        Ok(Err(error)) => error_response(id, "art_error", error.to_string()),
        Err(error) => error_response(id, "internal_error", error.to_string()),
    }
}

async fn playback(id: u64, context: &ServerContext, cmd: Cmd) -> ServerMessage {
    let Some(player) = &context.player else {
        return error_response(id, "unavailable", "playback is not configured");
    };
    match player.send_timed(cmd).await {
        Ok(()) => ok_response(id, None),
        Err(error) => play_error_response(id, error),
    }
}

async fn play_track(
    id: u64,
    context: &ServerContext,
    track_id: i64,
    scope: String,
) -> ServerMessage {
    let query_scope = scope.clone();
    play_queue(id, context, scope, move |db| {
        db.queue_for_scope(track_id, &query_scope)
    })
    .await
}

async fn play_album(id: u64, context: &ServerContext, album_id: i64) -> ServerMessage {
    play_queue(id, context, "album".into(), move |db| {
        let tracks = db.album_tracks(album_id)?;
        Ok((!tracks.is_empty()).then_some((tracks, 0)))
    })
    .await
}

/// Resolves a queue from the library, then hands it to the player.
async fn play_queue<F>(id: u64, context: &ServerContext, scope: String, resolve: F) -> ServerMessage
where
    F: FnOnce(&crate::db::Db) -> rusqlite::Result<Option<(Vec<crate::db::Track>, usize)>>
        + Send
        + 'static,
{
    let (Some(db_path), Some(player)) = (context.db_path.clone(), &context.player) else {
        return error_response(id, "unavailable", "playback is not configured");
    };
    let resolved = tokio::task::spawn_blocking(move || -> Result<_> {
        let db = crate::db::Db::open(db_path)?;
        Ok(resolve(&db)?)
    })
    .await;
    let (tracks, start) = match resolved {
        Ok(Ok(Some(queue))) => queue,
        Ok(Ok(None)) => return error_response(id, "not_found", "no such track or album"),
        Ok(Err(error)) => return error_response(id, "database_error", error.to_string()),
        Err(error) => return error_response(id, "internal_error", error.to_string()),
    };
    let items = tracks.iter().map(queue_item).collect();
    match player
        .send_timed(Cmd::PlayItems {
            items,
            start,
            scope,
        })
        .await
    {
        Ok(()) => ok_response(id, None),
        Err(error) => play_error_response(id, error),
    }
}

fn play_error_response(id: u64, error: PlayError) -> ServerMessage {
    error_response(id, &error.code, error.message)
}

async fn add_folder(id: u64, context: &ServerContext, path: String) -> ServerMessage {
    let Some(db_path) = context.db_path.clone() else {
        return error_response(id, "unavailable", "library database is not configured");
    };
    let folder =
        match tokio::time::timeout(Duration::from_secs(5), tokio::fs::canonicalize(&path)).await {
            Ok(Ok(folder)) => folder,
            Ok(Err(error)) => return error_response(id, "invalid_folder", error.to_string()),
            Err(_) => return error_response(id, "folder_timeout", "folder lookup timed out"),
        };
    match tokio::time::timeout(Duration::from_secs(5), tokio::fs::metadata(&folder)).await {
        Ok(Ok(metadata)) if metadata.is_dir() => {}
        Ok(Ok(_)) => {
            return error_response(id, "invalid_folder", "path is not a directory");
        }
        Ok(Err(error)) => return error_response(id, "invalid_folder", error.to_string()),
        Err(_) => return error_response(id, "folder_timeout", "folder lookup timed out"),
    }

    // Register the folder durably *before* answering, so it exists even if the
    // scan that follows is superseded or cancelled.
    let folder_path = folder.to_string_lossy().into_owned();
    let registered = {
        let db_path = db_path.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            let db = crate::db::Db::open(db_path)?;
            db.ensure_folder(&folder_path)?;
            Ok(db.list_folders()?.into_iter().map(|f| f.path).collect())
        })
        .await
    };
    match registered {
        Ok(Ok(folders)) => context.state.set_folders(folders).await,
        Ok(Err(error)) => return error_response(id, "database_error", error.to_string()),
        Err(error) => return error_response(id, "internal_error", error.to_string()),
    }

    spawn_scans(db_path, context.state.clone(), false);
    ok_response(
        id,
        Some(json!({ "path": folder.to_string_lossy(), "scanning": true })),
    )
}

async fn remove_folder(id: u64, context: &ServerContext, path: String) -> ServerMessage {
    let Some(db_path) = context.db_path.clone() else {
        return error_response(id, "unavailable", "library database is not configured");
    };
    // Cancel, then wait for any in-flight scan task to release the gate (it
    // holds it until it has stopped writing). Holding the gate ourselves keeps
    // a queued scan from starting until the folder is gone; a scan that was
    // cancelled before it started sees its flag and exits without touching the DB.
    let cancelled_scan = context.state.cancel_scan();
    let _scan_gate = context.state.acquire_scan_gate().await;
    context.state.finish_scan().await;
    let respawn_path = db_path.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<_> {
        let db = crate::db::Db::open(db_path)?;
        let removed = db.remove_folder(&path)?;
        let folders: Vec<String> = db
            .list_folders()?
            .into_iter()
            .map(|folder| folder.path)
            .collect();
        Ok((removed, folders))
    })
    .await;

    match result {
        Ok(Ok((removed, folders))) => {
            let remaining = !folders.is_empty();
            context.state.set_folders(folders).await;
            // The cancelled scan may have been indexing other folders; pick
            // their work back up (incremental, so finished files are skipped).
            // The new task queues behind `_scan_gate`, which we still hold.
            if cancelled_scan && remaining {
                spawn_scans(respawn_path, context.state.clone(), false);
            }
            ok_response(id, Some(json!({ "removed": removed })))
        }
        Ok(Err(error)) => error_response(id, "database_error", error.to_string()),
        Err(error) => error_response(id, "internal_error", error.to_string()),
    }
}

async fn rescan(id: u64, context: &ServerContext, full: bool) -> ServerMessage {
    let Some(db_path) = context.db_path.clone() else {
        return error_response(id, "unavailable", "library database is not configured");
    };
    let list_path = db_path.clone();
    let folders = tokio::task::spawn_blocking(move || -> Result<Vec<PathBuf>> {
        Ok(crate::db::Db::open(list_path)?
            .list_folders()?
            .into_iter()
            .map(|folder| PathBuf::from(folder.path))
            .collect())
    })
    .await;

    match folders {
        Ok(Ok(folders)) => {
            let count = folders.len();
            spawn_scans(db_path, context.state.clone(), full);
            ok_response(id, Some(json!({ "folders": count, "full": full })))
        }
        Ok(Err(error)) => error_response(id, "database_error", error.to_string()),
        Err(error) => error_response(id, "internal_error", error.to_string()),
    }
}

/// Ends the scan task's claim on the "active scan" slot, however it exits.
struct ActiveScan {
    state: AppState,
    cancel: Arc<AtomicBool>,
}

impl Drop for ActiveScan {
    fn drop(&mut self) {
        self.state.end_scan(&self.cancel);
    }
}

/// Queues a scan of *every registered folder*. The folder list is read when the
/// task gets the scan gate, not when it is queued, so a scan that supersedes
/// (cancels) an earlier one still covers the earlier one's remaining work and
/// any folder added in between. Incremental mode skips unchanged files.
fn spawn_scans(db_path: PathBuf, state: AppState, full: bool) {
    if full {
        state.request_full_scan();
    }
    let cancel = state.begin_scan();
    tokio::spawn(async move {
        let _scan_gate = state.acquire_scan_gate().await;
        let _active = ActiveScan {
            state: state.clone(),
            cancel: cancel.clone(),
        };
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let full = state.take_full_scan();
        let cancelled = run_scan_task(&db_path, &state, &cancel, full).await;
        if cancelled && full {
            // Cancelled before finishing: whoever scans next must stay full.
            state.request_full_scan();
        }
    });
}

/// Scans all registered folders. Returns `true` if it was cancelled.
async fn run_scan_task(
    db_path: &Path,
    state: &AppState,
    cancel: &Arc<AtomicBool>,
    full: bool,
) -> bool {
    let list_path = db_path.to_path_buf();
    let listed = tokio::task::spawn_blocking(move || -> Result<Vec<PathBuf>> {
        Ok(crate::db::Db::open(list_path)?
            .list_folders()?
            .into_iter()
            .map(|folder| PathBuf::from(folder.path))
            .collect())
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|listed| listed);
    let folders = match listed {
        Ok(folders) => folders,
        Err(error) => {
            state.set_scan_error(error.to_string()).await;
            return false;
        }
    };

    if folders.is_empty() {
        state
            .update_scan(scan::ScanProgress {
                scanning: false,
                scanned: 0,
                total: 0,
            })
            .await;
    }
    let mut first_error = None;
    let mut cancelled = false;
    for folder in folders {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let scan_path = db_path.to_path_buf();
        let scan_state = state.clone();
        let scan_cancel = cancel.clone();
        let runtime = tokio::runtime::Handle::current();
        let result = tokio::task::spawn_blocking(move || -> Result<_> {
            let db = crate::db::Db::open(scan_path)?;
            scan::run(db, &folder, full, scan_cancel, move |progress| {
                // Synchronous so no progress update can land after the scan
                // (and its gate) has finished.
                runtime.block_on(scan_state.update_scan(progress));
            })
        })
        .await;

        match result
            .map_err(anyhow::Error::from)
            .and_then(|result| result)
        {
            Ok(result) if result.cancelled => {
                cancelled = true;
                break;
            }
            Ok(_) => {}
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    break;
                }
                // One bad folder must not stop the others from being scanned.
                tracing::warn!(%error, "folder scan failed");
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
    }

    if let Ok(Ok(folders)) = tokio::task::spawn_blocking({
        let db_path = db_path.to_path_buf();
        move || crate::db::Db::open(db_path).and_then(|db| db.list_folders())
    })
    .await
    {
        state
            .set_folders(folders.into_iter().map(|folder| folder.path).collect())
            .await;
    }
    if let Some(message) = first_error {
        state.set_scan_error(message).await;
    }
    cancelled
}

fn ok_response(id: u64, result: Option<Value>) -> ServerMessage {
    ServerMessage::Response {
        id,
        ok: true,
        result,
        error: None,
    }
}

fn protocol_error_response(id: u64, error: ProtocolError) -> ServerMessage {
    ServerMessage::Response {
        id,
        ok: false,
        result: None,
        error: Some(error),
    }
}

fn error_response(id: u64, code: &str, message: impl Into<String>) -> ServerMessage {
    protocol_error_response(
        id,
        ProtocolError {
            code: code.into(),
            message: message.into(),
        },
    )
}

async fn write_message(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &ServerMessage,
) -> Result<()> {
    let mut value = serde_json::to_value(message)?;
    value
        .as_object_mut()
        .expect("server messages serialize as JSON objects")
        .insert("v".into(), json!(PROTOCOL_VERSION));
    let mut encoded = serde_json::to_vec(&value)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    Ok(())
}
