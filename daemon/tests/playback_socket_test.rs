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
#[allow(dead_code)]
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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use player::{Backend, PlayError, Player};
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Default)]
struct Log {
    played: Vec<PathBuf>,
    paused: bool,
    volume: f32,
    position_ms: u64,
}

struct FakeBackend(Arc<Mutex<Log>>);

impl Backend for FakeBackend {
    fn play(&mut self, path: &Path, start_ms: u64) -> Result<(), PlayError> {
        std::fs::File::open(path)
            .map_err(|error| PlayError::new("file_unavailable", error.to_string()))?;
        let mut log = self.0.lock().unwrap();
        log.played.push(path.to_path_buf());
        log.position_ms = start_ms;
        log.paused = false;
        Ok(())
    }
    fn pause(&mut self) {
        self.0.lock().unwrap().paused = true;
    }
    fn resume(&mut self) {
        self.0.lock().unwrap().paused = false;
    }
    fn stop(&mut self) {}
    fn seek(&mut self, position_ms: u64) -> Result<(), PlayError> {
        self.0.lock().unwrap().position_ms = position_ms;
        Ok(())
    }
    fn set_volume(&mut self, level: f32) {
        self.0.lock().unwrap().volume = level;
    }
    fn position_ms(&self) -> u64 {
        self.0.lock().unwrap().position_ms
    }
    fn finished(&self) -> bool {
        false
    }
}

fn track(folder_id: i64, path: &Path, title: &str, no: i64) -> db::TrackInput {
    db::TrackInput {
        folder_id,
        path: path.to_string_lossy().into_owned(),
        size: 1,
        mtime: 1,
        title: title.into(),
        artist: Some("Final Form".into()),
        album_artist: Some("Final Form".into()),
        album: Some("Phase One".into()),
        track_no: Some(no),
        disc_no: Some(1),
        year: None,
        genre: None,
        duration_ms: 180_000,
        has_art: false,
    }
}

struct Client {
    lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl Client {
    async fn call(&mut self, cmd: Value) -> Value {
        self.next_id += 1;
        let mut message = cmd;
        message["v"] = json!(1);
        message["id"] = json!(self.next_id);
        let mut line = serde_json::to_vec(&message).unwrap();
        line.push(b'\n');
        self.writer.write_all(&line).await.unwrap();
        loop {
            let value: Value =
                serde_json::from_str(&self.lines.next_line().await.unwrap().unwrap()).unwrap();
            if value["type"] == "response" && value["id"] == self.next_id {
                return value;
            }
        }
    }

    async fn state(&mut self) -> Value {
        self.call(json!({ "cmd": "get_state" })).await["result"].clone()
    }
}

#[tokio::test]
async fn play_next_pause_and_persist_over_the_socket_without_audio_hardware() {
    let temp = tempdir().unwrap();
    let paths: Vec<PathBuf> = (1..=3)
        .map(|n| {
            let path = temp.path().join(format!("{n}.flac"));
            std::fs::write(&path, b"x").unwrap();
            path
        })
        .collect();
    let db_path = temp.path().join("library.sqlite");
    let database = db::Db::open(&db_path).unwrap();
    let folder = database
        .upsert_folder(&temp.path().to_string_lossy(), None)
        .unwrap();
    let mut ids = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let no = index as i64 + 1;
        ids.push(
            database
                .upsert_track(&track(folder, path, &format!("Track {no}"), no))
                .unwrap(),
        );
    }
    let album_id = database.list_albums(10, 0).unwrap()[0].id;
    let artist_id = database.list_artists(10, 0).unwrap()[0].id;

    let app_state = state::AppState::new();
    let log = Arc::new(Mutex::new(Log::default()));
    let backend_log = log.clone();
    let persist = temp.path().join(server::PLAYBACK_FILE);
    let player = Player::spawn(
        move || Box::new(FakeBackend(backend_log)),
        app_state.clone(),
        tokio::runtime::Handle::current(),
        Some(persist.clone()),
        None,
    );
    let socket_path = temp.path().join("backend.sock");
    let listener = server::bind(&socket_path).unwrap();
    let task = tokio::spawn(server::serve_with_player(
        listener,
        app_state,
        db_path.clone(),
        player,
    ));

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, writer) = stream.into_split();
    let mut client = Client {
        lines: BufReader::new(reader).lines(),
        writer,
        next_id: 0,
    };

    for kind in ["tracks", "albums", "artists"] {
        let response = client.call(json!({ "cmd": "query", "kind": kind })).await;
        assert_eq!(response["ok"], true, "{response}");
        assert!(!response["result"].as_array().unwrap().is_empty());
    }
    let search = client
        .call(json!({ "cmd": "query", "kind": "search", "q": "Track 2" }))
        .await;
    assert_eq!(search["result"][0]["title"], "Track 2");
    let album = client
        .call(json!({ "cmd": "query", "kind": "album", "q": album_id.to_string() }))
        .await;
    assert_eq!(album["result"].as_array().unwrap().len(), 3);
    let artist = client
        .call(json!({ "cmd": "query", "kind": "artist", "q": artist_id.to_string() }))
        .await;
    assert_eq!(artist["result"].as_array().unwrap().len(), 3);
    let home = client.call(json!({ "cmd": "query", "kind": "home" })).await;
    assert_eq!(home["result"]["stats"]["tracks"], 3);
    let art = client
        .call(json!({ "cmd": "get_art", "album_id": album_id }))
        .await;
    assert_eq!(art["error"]["code"], "not_found");

    // play: album queue starting at the second track.
    let response = client
        .call(json!({ "cmd": "play", "track_id": ids[1] }))
        .await;
    assert_eq!(response["ok"], true, "{response}");
    let state = client.state().await;
    assert_eq!(state["playing"], true);
    assert_eq!(state["track"]["title"], "Track 2");
    assert_eq!(state["duration_ms"], 180_000);

    assert_eq!(client.call(json!({ "cmd": "next" })).await["ok"], true);
    assert_eq!(client.state().await["track"]["title"], "Track 3");
    assert_eq!(client.call(json!({ "cmd": "previous" })).await["ok"], true);
    assert_eq!(client.state().await["track"]["title"], "Track 2");

    // controls
    assert_eq!(
        client
            .call(json!({ "cmd": "seek", "position_ms": 30_000 }))
            .await["ok"],
        true
    );
    assert_eq!(
        client
            .call(json!({ "cmd": "set_volume", "level": 0.5 }))
            .await["ok"],
        true
    );
    assert_eq!(
        client
            .call(json!({ "cmd": "set_shuffle", "enabled": true }))
            .await["ok"],
        true
    );
    assert_eq!(
        client
            .call(json!({ "cmd": "set_repeat", "mode": "all" }))
            .await["ok"],
        true
    );
    let bad = client
        .call(json!({ "cmd": "set_repeat", "mode": "twice" }))
        .await;
    assert_eq!(bad["error"]["code"], "invalid_argument");

    // pause persists the track and position.
    assert_eq!(client.call(json!({ "cmd": "pause" })).await["ok"], true);
    let state = client.state().await;
    assert_eq!(state["playing"], false);
    assert_eq!(state["position_ms"], 30_000);
    assert_eq!(state["volume"], 0.5);
    assert_eq!(state["shuffle"], true);
    assert_eq!(state["repeat"], "all");
    assert!(log.lock().unwrap().paused);
    let saved = player::load_playback(&persist).unwrap();
    assert_eq!((saved.track_id, saved.position_ms), (ids[1], 30_000));

    // A restarted daemon can rebuild the queue from the saved file.
    let restore = server::load_restore(&db_path, &persist).unwrap();
    assert_eq!(restore.position_ms, 30_000);
    assert_eq!(restore.items[restore.start].id, ids[1]);

    // toggle resumes
    assert_eq!(client.call(json!({ "cmd": "toggle" })).await["ok"], true);
    assert_eq!(client.state().await["playing"], true);

    // play_album plays the album from the first track
    let response = client
        .call(json!({ "cmd": "play_album", "album_id": album_id }))
        .await;
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(client.state().await["track"]["title"], "Track 1");

    // a missing file reports file_unavailable both as RPC error and in state
    std::fs::remove_file(&paths[2]).unwrap();
    let response = client
        .call(json!({ "cmd": "play", "track_id": ids[2] }))
        .await;
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "file_unavailable");
    let state = client.state().await;
    assert_eq!(state["playing"], false);
    assert_eq!(state["error_code"], "file_unavailable");

    // unknown ids are not_found
    let response = client
        .call(json!({ "cmd": "play", "track_id": 9999 }))
        .await;
    assert_eq!(response["error"]["code"], "not_found");

    task.abort();
}

#[tokio::test]
async fn playback_commands_without_a_player_report_unavailable() {
    let temp = tempdir().unwrap();
    let socket_path = temp.path().join("backend.sock");
    let listener = server::bind(&socket_path).unwrap();
    let task = tokio::spawn(server::serve(listener, state::AppState::new()));
    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, writer) = stream.into_split();
    let mut client = Client {
        lines: BufReader::new(reader).lines(),
        writer,
        next_id: 0,
    };
    let response = client.call(json!({ "cmd": "toggle" })).await;
    assert_eq!(response["error"]["code"], "unavailable");
    task.abort();
}

#[tokio::test]
async fn play_scope_keeps_album_and_artist_queues_and_opens_the_library() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("library.sqlite");
    let database = db::Db::open(&db_path).unwrap();
    let folder = database
        .upsert_folder(&temp.path().to_string_lossy(), None)
        .unwrap();
    let id = |title: &str, artist: &str, album: &str, no: i64| {
        let path = temp.path().join(format!("{title}.flac"));
        std::fs::write(&path, b"x").unwrap();
        let mut input = track(folder, &path, title, no);
        input.artist = Some(artist.into());
        input.album_artist = Some(artist.into());
        input.album = Some(album.into());
        database.upsert_track(&input).unwrap()
    };
    let ada_one = id("Ada One", "Ada", "Alpha", 1);
    let ada_two = id("Ada Two", "Ada", "Alpha", 2);
    let _ada_beta = id("Ada Beta", "Ada", "Beta", 1);
    let _bea = id("Bea One", "Bea", "Gamma", 1);
    drop(database);

    let app_state = state::AppState::new();
    let player = Player::spawn(
        || Box::new(FakeBackend(Arc::new(Mutex::new(Log::default())))),
        app_state.clone(),
        tokio::runtime::Handle::current(),
        Some(temp.path().join(server::PLAYBACK_FILE)),
        None,
    );
    let socket_path = temp.path().join("backend.sock");
    let listener = server::bind(&socket_path).unwrap();
    let task = tokio::spawn(server::serve_with_player(
        listener,
        app_state,
        db_path.clone(),
        player,
    ));
    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (reader, writer) = stream.into_split();
    let mut client = Client {
        lines: BufReader::new(reader).lines(),
        writer,
        next_id: 0,
    };

    // Album play stays on that album.
    assert_eq!(
        client
            .call(json!({ "cmd": "play", "track_id": ada_one, "scope": "album" }))
            .await["ok"],
        true
    );
    assert_eq!(client.call(json!({ "cmd": "next" })).await["ok"], true);
    assert_eq!(client.state().await["track"]["title"], "Ada Two");
    assert_eq!(client.call(json!({ "cmd": "next" })).await["ok"], true);
    assert_eq!(client.state().await["track"]["title"], "Ada Two");

    // Artist play continues onto the artist's other album.
    assert_eq!(
        client
            .call(json!({ "cmd": "play", "track_id": ada_two, "scope": "artist" }))
            .await["ok"],
        true
    );
    assert_eq!(client.call(json!({ "cmd": "next" })).await["ok"], true);
    assert_eq!(client.state().await["track"]["title"], "Ada Beta");

    // Library play continues across artists. Title order after Ada Two is Bea One.
    assert_eq!(
        client
            .call(json!({ "cmd": "play", "track_id": ada_two, "scope": "library" }))
            .await["ok"],
        true
    );
    assert_eq!(client.call(json!({ "cmd": "next" })).await["ok"], true);
    assert_eq!(client.state().await["track"]["title"], "Bea One");

    let restore = server::load_restore(&db_path, &temp.path().join(server::PLAYBACK_FILE)).unwrap();
    assert_eq!(restore.items.len(), 4);
    assert_eq!(restore.items[restore.start].meta["title"], "Bea One");

    task.abort();
}
