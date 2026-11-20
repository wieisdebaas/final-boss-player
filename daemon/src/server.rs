use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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

/// Rebuilds the last queue (the track's album) paused at the saved position.
pub fn load_restore(db_path: &Path, playback_path: &Path) -> Option<Restore> {
    let saved = player::load_playback(playback_path)?;
    let db = crate::db::Db::open(db_path).ok()?;
    let (tracks, start) = db.queue_for_track(saved.track_id).ok()??;
    Some(Restore {
        items: tracks.iter().map(queue_item).collect(),
        start,
        position_ms: saved.position_ms,
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
        Command::Play { track_id } => play_track(request.id, context, track_id).await,
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
    match player.send(cmd).await {
        Ok(()) => ok_response(id, None),
        Err(error) => play_error_response(id, error),
    }
}

async fn play_track(id: u64, context: &ServerContext, track_id: i64) -> ServerMessage {
    play_queue(id, context, move |db| db.queue_for_track(track_id)).await
}

async fn play_album(id: u64, context: &ServerContext, album_id: i64) -> ServerMessage {
    play_queue(id, context, move |db| {
        let tracks = db.album_tracks(album_id)?;
        Ok((!tracks.is_empty()).then_some((tracks, 0)))
    })
    .await
}

/// Resolves a queue from the library, then hands it to the player.
async fn play_queue<F>(id: u64, context: &ServerContext, resolve: F) -> ServerMessage
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
    match player.send(Cmd::PlayItems { items, start }).await {
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

    spawn_scans(db_path, context.state.clone(), vec![folder.clone()], false);
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
    context.state.cancel_scan();
    let _scan_gate = context.state.acquire_scan_gate().await;
    context.state.finish_scan().await;
    let result = tokio::task::spawn_blocking(move || -> Result<_> {
        let db = crate::db::Db::open(db_path)?;
        let removed = db.remove_folder(&path)?;
        let folders = db
            .list_folders()?
            .into_iter()
            .map(|folder| folder.path)
            .collect();
        Ok((removed, folders))
    })
    .await;

    match result {
        Ok(Ok((removed, folders))) => {
            context.state.set_folders(folders).await;
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
            spawn_scans(db_path, context.state.clone(), folders, full);
            ok_response(id, Some(json!({ "folders": count, "full": full })))
        }
        Ok(Err(error)) => error_response(id, "database_error", error.to_string()),
        Err(error) => error_response(id, "internal_error", error.to_string()),
    }
}

fn spawn_scans(db_path: PathBuf, state: AppState, folders: Vec<PathBuf>, full: bool) {
    let cancel = state.begin_scan();
    tokio::spawn(async move {
        let _scan_gate = state.acquire_scan_gate().await;
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if folders.is_empty() {
            state
                .update_scan(scan::ScanProgress {
                    scanning: false,
                    scanned: 0,
                    total: 0,
                })
                .await;
        }
        for folder in folders {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let scan_path = db_path.clone();
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
                Ok(result) if result.cancelled => break,
                Ok(_) => {}
                Err(error) => {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                    state.set_scan_error(error.to_string()).await;
                    return;
                }
            }
        }

        if let Ok(db) = crate::db::Db::open(&db_path) {
            if let Ok(folders) = db.list_folders() {
                state
                    .set_folders(folders.into_iter().map(|folder| folder.path).collect())
                    .await;
            }
        }
    });
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
