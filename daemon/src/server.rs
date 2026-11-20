use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::protocol::{
    parse_client_message, Command, ProtocolError, ServerMessage, PROTOCOL_VERSION,
};
use crate::state::AppState;

pub fn bind(socket_path: &Path) -> Result<UnixListener> {
    if socket_path.exists() {
        fs::remove_file(socket_path)
            .with_context(|| format!("could not remove stale socket {}", socket_path.display()))?;
    }

    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("could not bind {}", socket_path.display()))?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not secure {}", socket_path.display()))?;
    Ok(listener)
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
                        let snapshot = state.snapshot().await;
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
    let mut encoded = serde_json::to_vec(message)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    Ok(())
}
