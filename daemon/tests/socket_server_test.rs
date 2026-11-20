#[path = "../src/protocol.rs"]
mod protocol;
#[allow(dead_code)]
#[path = "../src/server.rs"]
mod server;
#[allow(dead_code)]
#[path = "../src/state.rs"]
mod state;

use std::os::unix::fs::{FileTypeExt, PermissionsExt};

use protocol::ServerMessage;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[tokio::test]
async fn sends_state_snapshot_then_answers_ping() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("backend.sock");
    let listener = server::bind(&socket_path).unwrap();
    let task = tokio::spawn(server::serve(listener, state::AppState::new()));

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let snapshot: ServerMessage =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    match snapshot {
        ServerMessage::Event { event, state } => {
            assert_eq!(event, "state_changed");
            assert_eq!(state.unwrap(), state::initial_player_state());
        }
        message => panic!("expected state_changed event, got {message:?}"),
    }

    writer
        .write_all(b"{\"v\":1,\"id\":42,\"cmd\":\"ping\"}\n")
        .await
        .unwrap();
    let response: ServerMessage =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(
        response,
        ServerMessage::Response {
            id: 42,
            ok: true,
            result: None,
            error: None,
        }
    );

    task.abort();
}

#[tokio::test]
async fn replaces_stale_socket_with_user_only_permissions() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("backend.sock");
    drop(std::os::unix::net::UnixListener::bind(&socket_path).unwrap());

    let _listener = server::bind(&socket_path).unwrap();
    let metadata = std::fs::metadata(&socket_path).unwrap();

    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
}

#[tokio::test]
async fn rejects_unsupported_commands() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("backend.sock");
    let listener = server::bind(&socket_path).unwrap();
    let task = tokio::spawn(server::serve(listener, state::AppState::new()));

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    lines.next_line().await.unwrap().unwrap();

    writer
        .write_all(b"{\"v\":1,\"id\":9,\"cmd\":\"pause\"}\n")
        .await
        .unwrap();
    let response: serde_json::Value =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();

    assert_eq!(response["id"], 9);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "unknown_command");

    task.abort();
}
