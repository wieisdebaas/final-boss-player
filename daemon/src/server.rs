use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

use crate::protocol::{
    parse_client_message, Command, ProtocolError, ServerMessage, PROTOCOL_VERSION,
};
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
    loop {
        let (stream, _) = listener.accept().await.context("socket accept failed")?;
        let client_state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_client(stream, client_state).await {
                tracing::debug!(%error, "socket client disconnected");
            }
        });
    }
}

async fn handle_client(stream: UnixStream, state: AppState) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut changes = state.subscribe();

    write_message(
        &mut writer,
        &ServerMessage::Event {
            event: "state_changed".into(),
            state: Some(state.snapshot().await),
        },
    )
    .await?;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line.context("could not read client message")? else {
                    return Ok(());
                };
                let response = dispatch(&line, &state).await;
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
                        let snapshot = recover_after_lag(&mut changes, &state).await;
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
    loop {
        match changes.try_recv() {
            Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed) => {
                break
            }
        }
    }
    state.snapshot().await
}

async fn dispatch(line: &str, state: &AppState) -> ServerMessage {
    let raw: Value = match serde_json::from_str(line) {
        Ok(raw) => raw,
        Err(error) => return error_response(0, "invalid_request", error.to_string()),
    };
    let id = raw.get("id").and_then(Value::as_u64).unwrap_or(0);
    let command = raw.get("cmd").and_then(Value::as_str);

    if !matches!(command, Some("hello" | "ping" | "get_state")) {
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
            Some(serde_json::to_value(state.snapshot().await).expect("state is serializable")),
        ),
        _ => error_response(request.id, "unknown_command", "command is not supported"),
    }
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
