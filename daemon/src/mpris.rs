//! Thin MPRIS (org.mpris.MediaPlayer2) adapter so media keys and `playerctl`
//! work while the panel is closed.
//!
//! Supported: Play, Pause, PlayPause, Stop (pauses), Next, Previous, Seek,
//! SetPosition, plus read-only PlaybackStatus / Metadata / Position / Volume.
//! Not supported (advertised as `false`): TrackList, Playlists, Raise/Quit,
//! LoopStatus/Shuffle/Rate writes, and the `Seeked` signal.
//!
//! The adapter is optional: if no session bus is reachable the daemon logs a
//! warning and keeps serving its own socket.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::sync::broadcast::error::RecvError;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, Value};
use zbus::{fdo, interface, Connection};

use crate::player::{Cmd, Player};
use crate::protocol::PlayerState;
use crate::state::AppState;

pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.endboss";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

/// What a transport request should do given the current state.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Send(Cmd),
    Nothing,
}

pub fn playback_status(state: &PlayerState) -> &'static str {
    if state.playing {
        "Playing"
    } else if state.track.is_some() {
        "Paused"
    } else {
        "Stopped"
    }
}

pub fn play_action(state: &PlayerState) -> Action {
    if state.playing {
        Action::Nothing
    } else {
        Action::Send(Cmd::Toggle)
    }
}

pub fn pause_action(state: &PlayerState) -> Action {
    if state.playing {
        Action::Send(Cmd::Pause)
    } else {
        Action::Nothing
    }
}

/// MPRIS `Seek` takes a signed microsecond offset; the daemon wants an
/// absolute millisecond position.
pub fn seek_target_ms(state: &PlayerState, offset_us: i64) -> u64 {
    let current = i64::try_from(state.position_ms).unwrap_or(i64::MAX);
    let target = current.saturating_add(offset_us / 1000).max(0);
    let target = u64::try_from(target).unwrap_or(0);
    match state.duration_ms {
        Some(duration) if duration > 0 => target.min(duration),
        _ => target,
    }
}

fn track_id(id: i64) -> String {
    format!("/org/endboss/track/{id}")
}

/// MPRIS `Metadata` for the current track (empty when nothing is loaded).
pub fn metadata(state: &PlayerState, art_dir: Option<&Path>) -> HashMap<String, Value<'static>> {
    let mut map: HashMap<String, Value<'static>> = HashMap::new();
    let Some(track) = state.track.as_ref() else {
        map.insert(
            "mpris:trackid".into(),
            Value::from(ObjectPath::from_static_str_unchecked(NO_TRACK)),
        );
        return map;
    };

    let id = track.get("id").and_then(serde_json::Value::as_i64);
    let path = id.map_or_else(|| NO_TRACK.to_string(), track_id);
    if let Ok(path) = ObjectPath::try_from(path) {
        map.insert("mpris:trackid".into(), Value::from(path));
    }
    let text = |key: &str| {
        track
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    if let Some(title) = text("title") {
        map.insert("xesam:title".into(), Value::from(title));
    }
    if let Some(artist) = text("artist") {
        map.insert("xesam:artist".into(), Value::from(vec![artist]));
    }
    if let Some(album) = text("album") {
        map.insert("xesam:album".into(), Value::from(album));
    }
    if let Some(album_artist) = text("album_artist") {
        map.insert("xesam:albumArtist".into(), Value::from(vec![album_artist]));
    }
    if let Some(duration) = state.duration_ms {
        let micros = i64::try_from(duration.saturating_mul(1000)).unwrap_or(i64::MAX);
        map.insert("mpris:length".into(), Value::from(micros));
    }
    if let (Some(dir), Some(album_id)) = (
        art_dir,
        track.get("album_id").and_then(serde_json::Value::as_i64),
    ) {
        let cached: PathBuf = crate::art::cache_path(dir, album_id);
        if cached.exists() {
            map.insert(
                "mpris:artUrl".into(),
                Value::from(format!("file://{}", cached.display())),
            );
        }
    }
    map
}

#[derive(Clone)]
struct Controls {
    state: AppState,
    player: Player,
    art_dir: Option<PathBuf>,
}

impl Controls {
    async fn run(&self, action: Action) -> fdo::Result<()> {
        match action {
            Action::Nothing => Ok(()),
            Action::Send(cmd) => self
                .player
                .send_timed(cmd)
                .await
                .map_err(|error| fdo::Error::Failed(error.to_string())),
        }
    }
}

struct Root;

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Endboss"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct PlayerIface(Controls);

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerIface {
    async fn next(&self) -> fdo::Result<()> {
        self.0.run(Action::Send(Cmd::Next)).await
    }
    async fn previous(&self) -> fdo::Result<()> {
        self.0.run(Action::Send(Cmd::Previous)).await
    }
    async fn pause(&self) -> fdo::Result<()> {
        let state = self.0.state.snapshot().await;
        self.0.run(pause_action(&state)).await
    }
    async fn play_pause(&self) -> fdo::Result<()> {
        self.0.run(Action::Send(Cmd::Toggle)).await
    }
    async fn stop(&self) -> fdo::Result<()> {
        let state = self.0.state.snapshot().await;
        self.0.run(pause_action(&state)).await
    }
    async fn play(&self) -> fdo::Result<()> {
        let state = self.0.state.snapshot().await;
        self.0.run(play_action(&state)).await
    }
    async fn seek(&self, offset_us: i64) -> fdo::Result<()> {
        let state = self.0.state.snapshot().await;
        let target = seek_target_ms(&state, offset_us);
        self.0.run(Action::Send(Cmd::Seek(target))).await
    }
    async fn set_position(&self, _track_id: ObjectPath<'_>, position_us: i64) -> fdo::Result<()> {
        if position_us < 0 {
            return Ok(());
        }
        let target = u64::try_from(position_us / 1000).unwrap_or(0);
        self.0.run(Action::Send(Cmd::Seek(target))).await
    }
    fn open_uri(&self, _uri: &str) {}

    #[zbus(property)]
    async fn playback_status(&self) -> String {
        playback_status(&self.0.state.snapshot().await).to_string()
    }
    #[zbus(property)]
    async fn metadata(&self) -> HashMap<String, Value<'static>> {
        metadata(&self.0.state.snapshot().await, self.0.art_dir.as_deref())
    }
    #[zbus(property)]
    async fn position(&self) -> i64 {
        let state = self.0.state.snapshot().await;
        i64::try_from(state.position_ms.saturating_mul(1000)).unwrap_or(i64::MAX)
    }
    #[zbus(property)]
    async fn volume(&self) -> f64 {
        f64::from(self.0.state.snapshot().await.volume)
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

async fn register(controls: Controls) -> zbus::Result<Connection> {
    zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, Root)?
        .serve_at(OBJECT_PATH, PlayerIface(controls))?
        .build()
        .await
}

/// Starts the MPRIS service in the background. Never fails the daemon: a
/// missing session bus only disables media-key integration.
pub fn spawn(state: AppState, player: Player, art_dir: Option<PathBuf>) {
    tokio::spawn(async move {
        let controls = Controls {
            state: state.clone(),
            player,
            art_dir,
        };
        let connection = match register(controls).await {
            Ok(connection) => connection,
            Err(error) => {
                tracing::warn!(%error, "MPRIS unavailable; media keys disabled");
                return;
            }
        };
        tracing::info!(name = BUS_NAME, "MPRIS service registered");
        forward_changes(state, connection).await;
    });
}

/// (status, track id, title, duration) — what clients are told about.
type ChangeKey = (&'static str, Option<i64>, Option<String>, Option<u64>);

/// Emits PropertiesChanged when status or metadata change. Position is read
/// on demand by clients (per spec it does not emit changes while playing).
async fn forward_changes(state: AppState, connection: Connection) {
    let Ok(iface) = connection
        .object_server()
        .interface::<_, PlayerIface>(OBJECT_PATH)
        .await
    else {
        return;
    };
    let mut changes = state.subscribe();
    let mut last: Option<ChangeKey> = None;
    loop {
        let snapshot = match changes.recv().await {
            Ok(snapshot) => snapshot,
            Err(RecvError::Lagged(_)) => state.snapshot().await,
            Err(RecvError::Closed) => return,
        };
        let key = (
            playback_status(&snapshot),
            snapshot
                .track
                .as_ref()
                .and_then(|track| track.get("id"))
                .and_then(serde_json::Value::as_i64),
            snapshot
                .track
                .as_ref()
                .and_then(|track| track.get("title"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            snapshot.duration_ms,
        );
        if last.as_ref() == Some(&key) {
            continue;
        }
        let status_changed = last.as_ref().is_none_or(|previous| previous.0 != key.0);
        let metadata_changed = last.as_ref().is_none_or(|previous| {
            previous.1 != key.1 || previous.2 != key.2 || previous.3 != key.3
        });
        last = Some(key);

        let emitter: &SignalEmitter<'_> = iface.signal_emitter();
        let this = iface.get().await;
        if status_changed {
            let _ = this.playback_status_changed(emitter).await;
        }
        if metadata_changed {
            let _ = this.metadata_changed(emitter).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::initial_player_state;
    use serde_json::json;

    fn playing_state() -> PlayerState {
        let mut state = initial_player_state();
        state.playing = true;
        state.position_ms = 10_000;
        state.duration_ms = Some(180_000);
        state.track = Some(json!({
            "id": 7,
            "title": "Final Form",
            "artist": "Endboss",
            "album": "Phase One",
            "album_id": 3
        }));
        state
    }

    #[test]
    fn status_reflects_playback() {
        let mut state = initial_player_state();
        assert_eq!(playback_status(&state), "Stopped");
        state = playing_state();
        assert_eq!(playback_status(&state), "Playing");
        state.playing = false;
        assert_eq!(playback_status(&state), "Paused");
    }

    #[test]
    fn play_and_pause_are_idempotent() {
        let mut state = playing_state();
        assert_eq!(play_action(&state), Action::Nothing);
        assert_eq!(pause_action(&state), Action::Send(Cmd::Pause));
        state.playing = false;
        assert_eq!(play_action(&state), Action::Send(Cmd::Toggle));
        assert_eq!(pause_action(&state), Action::Nothing);
    }

    #[test]
    fn seek_offset_is_relative_and_clamped() {
        let state = playing_state();
        assert_eq!(seek_target_ms(&state, 5_000_000), 15_000);
        assert_eq!(seek_target_ms(&state, -30_000_000), 0);
        assert_eq!(seek_target_ms(&state, 999_000_000_000), 180_000);
    }

    #[test]
    fn metadata_maps_track_fields() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(crate::art::cache_path(dir.path(), 3), b"jpg").unwrap();
        let map = metadata(&playing_state(), Some(dir.path()));
        assert_eq!(
            map["mpris:trackid"],
            Value::from(ObjectPath::try_from("/org/endboss/track/7").unwrap())
        );
        assert_eq!(map["xesam:title"], Value::from("Final Form".to_string()));
        assert_eq!(
            map["xesam:artist"],
            Value::from(vec!["Endboss".to_string()])
        );
        assert_eq!(map["xesam:album"], Value::from("Phase One".to_string()));
        assert_eq!(map["mpris:length"], Value::from(180_000_000_i64));
        let Value::Str(url) = &map["mpris:artUrl"] else {
            panic!("artUrl should be a string");
        };
        assert!(url.starts_with("file://") && url.ends_with("album-3.jpg"));
    }

    #[test]
    fn metadata_omits_art_when_not_cached_and_when_idle() {
        let dir = tempfile::tempdir().unwrap();
        let map = metadata(&playing_state(), Some(dir.path()));
        assert!(!map.contains_key("mpris:artUrl"));

        let idle = metadata(&initial_player_state(), None);
        assert_eq!(idle.len(), 1);
        assert!(idle.contains_key("mpris:trackid"));
    }
}
