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
    serve_context(listener, ServerContext::new(state, None)).await
}

pub async fn run_with_db(socket_path: &Path, db_path: PathBuf, state: AppState) -> Result<()> {
    serve_context(bind(socket_path)?, ServerContext::new(state, Some(db_path))).await
}

#[derive(Clone)]
struct ServerContext {
    state: AppState,
    db_path: Option<PathBuf>,
}

impl ServerContext {
    fn new(state: AppState, db_path: Option<PathBuf>) -> Self {
        Self { state, db_path }
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
        Some("hello" | "ping" | "get_state" | "add_folder" | "remove_folder" | "rescan")
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
        _ => error_response(request.id, "unknown_command", "command is not supported"),
    }
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
    context.state.cancel_scan();
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
                    let progress_state = scan_state.clone();
                    runtime.spawn(async move {
                        progress_state.update_scan(progress).await;
                    });
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
