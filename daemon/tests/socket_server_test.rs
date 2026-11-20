#[allow(dead_code)]
#[path = "../src/art.rs"]
mod art;
#[allow(dead_code)]
#[path = "../src/db.rs"]
mod db;
#[allow(dead_code)]
#[path = "../src/meta.rs"]
mod meta;
#[allow(dead_code)]
#[path = "../src/mpris.rs"]
mod mpris;
#[allow(dead_code)]
#[path = "../src/player.rs"]
mod player;
#[path = "../src/protocol.rs"]
mod protocol;
#[allow(dead_code)]
#[path = "../src/scan.rs"]
mod scan;
#[allow(dead_code)]
#[path = "../src/server.rs"]
mod server;
#[allow(dead_code)]
#[path = "../src/state.rs"]
mod state;

use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixListener as StdUnixListener;

use protocol::{PlayerState, ServerMessage};
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

    let snapshot_line = lines.next_line().await.unwrap().unwrap();
    let snapshot_json: serde_json::Value = serde_json::from_str(&snapshot_line).unwrap();
    assert_eq!(snapshot_json["v"], 1);
    let snapshot: ServerMessage = serde_json::from_str(&snapshot_line).unwrap();
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
    let response_line = lines.next_line().await.unwrap().unwrap();
    let response_json: serde_json::Value = serde_json::from_str(&response_line).unwrap();
    assert_eq!(response_json["v"], 1);
    let response: ServerMessage = serde_json::from_str(&response_line).unwrap();
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

#[test]
fn refuses_to_replace_a_regular_file() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("backend.sock");
    std::fs::write(&socket_path, "keep me").unwrap();

    assert!(server::bind(&socket_path).is_err());
    assert_eq!(std::fs::read_to_string(&socket_path).unwrap(), "keep me");
}

#[test]
fn refuses_to_replace_an_active_socket() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("backend.sock");
    let active_listener = StdUnixListener::bind(&socket_path).unwrap();

    assert!(server::bind(&socket_path).is_err());
    assert!(active_listener.local_addr().is_ok());
    assert!(std::fs::symlink_metadata(&socket_path)
        .unwrap()
        .file_type()
        .is_socket());
}

#[tokio::test]
async fn lag_recovery_discards_retained_snapshots() {
    let state = state::AppState::new();
    let mut changes = state.subscribe();

    for generation in 1..=20 {
        state.replace(player_state(generation)).await;
    }
    assert!(matches!(
        changes.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
    ));

    let recovered = server::recover_after_lag(&mut changes, &state).await;

    assert_eq!(recovered.generation, 20);
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
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
        .write_all(b"{\"v\":1,\"id\":9,\"cmd\":\"dance\"}\n")
        .await
        .unwrap();
    let response: serde_json::Value =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();

    assert_eq!(response["id"], 9);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "unknown_command");

    task.abort();
}

fn player_state(generation: u64) -> PlayerState {
    PlayerState {
        generation,
        ..state::initial_player_state()
    }
}

#[tokio::test]
async fn remove_folder_waits_for_scan_and_leaves_no_trace() {
    let temp = tempdir().unwrap();
    let music = temp.path().join("music");
    std::fs::create_dir(&music).unwrap();
    for index in 0..5 {
        write_wave(&music.join(format!("track {index}.wav")));
    }
    let socket_path = temp.path().join("backend.sock");
    let db_path = temp.path().join("library.sqlite");
    let server_socket = socket_path.clone();
    let server_db = db_path.clone();
    let task = tokio::spawn(async move {
        server::run_with_db(&server_socket, server_db, state::AppState::new()).await
    });
    for _ in 0..200 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // Remove immediately after each add, while the scan is queued or running.
    for id in 1..=5_u64 {
        let add = format!(
            "{{\"v\":1,\"id\":{id},\"cmd\":\"add_folder\",\"path\":{}}}\n",
            serde_json::to_string(&music.to_string_lossy()).unwrap()
        );
        writer.write_all(add.as_bytes()).await.unwrap();
        let remove = format!(
            "{{\"v\":1,\"id\":{},\"cmd\":\"remove_folder\",\"path\":{}}}\n",
            id + 100,
            serde_json::to_string(&music.canonicalize().unwrap().to_string_lossy()).unwrap()
        );
        writer.write_all(remove.as_bytes()).await.unwrap();
        let remove_id = id + 100;
        loop {
            let value: serde_json::Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            if value["id"] == remove_id {
                assert_eq!(value["ok"], true, "{value}");
                break;
            }
        }

        // Once remove_folder has answered, no stale scan may bring the folder back.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let db = db::Db::open(&db_path).unwrap();
        let stats = db.stats().unwrap();
        assert_eq!((stats.folders, stats.tracks), (0, 0), "round {id}");
    }

    task.abort();
}

fn write_wave(path: &std::path::Path) {
    let data = vec![128_u8; 8_000];
    let mut wave = Vec::with_capacity(44 + data.len());
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36_u32 + data.len() as u32).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&8_000_u32.to_le_bytes());
    wave.extend_from_slice(&8_000_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&8_u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wave.extend_from_slice(&data);
    std::fs::write(path, wave).unwrap();
}

#[tokio::test]
async fn scan_gate_makes_removal_wait_for_active_scan() {
    let state = state::AppState::new();
    let gate = state.acquire_scan_gate().await;
    let waiting = state.clone();
    let mut waiter = tokio::spawn(async move {
        let _gate = waiting.acquire_scan_gate().await;
    });

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut waiter)
            .await
            .is_err()
    );
    drop(gate);
    tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .unwrap()
        .unwrap();
}
