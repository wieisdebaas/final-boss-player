#[allow(dead_code)]
#[path = "../src/db.rs"]
mod db;
#[allow(dead_code)]
#[path = "../src/meta.rs"]
mod meta;
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
#[path = "../src/state.rs"]
mod state;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use player::{Backend, Engine, PlayError, Queue, QueueItem, Repeat};
use tempfile::tempdir;

fn item(id: i64, path: &str) -> QueueItem {
    QueueItem {
        id,
        path: PathBuf::from(path),
        duration_ms: 200_000,
        meta: serde_json::json!({ "id": id, "title": format!("track {id}") }),
    }
}

fn items(count: i64) -> Vec<QueueItem> {
    (1..=count)
        .map(|id| item(id, &format!("/t/{id}")))
        .collect()
}

fn current_id(queue: &Queue) -> i64 {
    queue.current().unwrap().id
}

// ---------------------------------------------------------------- queue

#[test]
fn queue_walks_forward_and_stops_at_the_end() {
    let mut queue = Queue::new(7);
    queue.set(items(3), 0);
    assert_eq!(current_id(&queue), 1);
    assert_eq!(queue.next(false).unwrap().id, 2);
    assert_eq!(queue.next(false).unwrap().id, 3);
    assert!(queue.next(false).is_none());
    // Reaching the end leaves the last track selected.
    assert_eq!(current_id(&queue), 3);
}

#[test]
fn queue_starts_at_the_requested_index_and_walks_back() {
    let mut queue = Queue::new(7);
    queue.set(items(4), 2);
    assert_eq!(current_id(&queue), 3);
    assert_eq!(queue.previous().unwrap().id, 2);
    assert_eq!(queue.previous().unwrap().id, 1);
    // At the start without repeat the first track stays selected.
    assert_eq!(queue.previous().unwrap().id, 1);
}

#[test]
fn queue_repeat_all_wraps_in_both_directions() {
    let mut queue = Queue::new(7);
    queue.set(items(3), 2);
    queue.set_repeat(Repeat::All);
    assert_eq!(queue.next(false).unwrap().id, 1);
    assert_eq!(queue.previous().unwrap().id, 3);
}

#[test]
fn queue_repeat_one_only_repeats_on_automatic_advance() {
    let mut queue = Queue::new(7);
    queue.set(items(3), 1);
    queue.set_repeat(Repeat::One);
    assert_eq!(queue.next(true).unwrap().id, 2);
    assert_eq!(queue.next(true).unwrap().id, 2);
    assert_eq!(queue.next(false).unwrap().id, 3);
}

#[test]
fn queue_shuffle_visits_every_track_once_starting_with_current() {
    let mut queue = Queue::new(42);
    queue.set(items(8), 3);
    queue.set_shuffle(true);
    assert_eq!(current_id(&queue), 4);

    let mut seen = vec![current_id(&queue)];
    while let Some(next) = queue.next(true) {
        seen.push(next.id);
    }
    assert_eq!(seen.len(), 8);
    let mut sorted = seen.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (1..=8).collect::<Vec<_>>());
}

#[test]
fn queue_disabling_shuffle_continues_in_album_order_from_current() {
    let mut queue = Queue::new(42);
    queue.set(items(6), 0);
    queue.set_shuffle(true);
    let landed = queue.next(false).unwrap().id;
    queue.set_shuffle(false);
    assert_eq!(current_id(&queue), landed);
    if landed < 6 {
        assert_eq!(queue.next(false).unwrap().id, landed + 1);
    }
}

#[test]
fn queue_rejects_empty_or_out_of_range_start() {
    let mut queue = Queue::new(1);
    assert!(!queue.set(Vec::new(), 0));
    assert!(!queue.set(items(2), 2));
    assert!(queue.current().is_none());
    assert!(queue.next(false).is_none());
    assert!(queue.previous().is_none());
}

#[test]
fn repeat_modes_parse_and_print() {
    assert_eq!(Repeat::parse("off"), Some(Repeat::Off));
    assert_eq!(Repeat::parse("all"), Some(Repeat::All));
    assert_eq!(Repeat::parse("one"), Some(Repeat::One));
    assert_eq!(Repeat::parse("loop"), None);
    assert_eq!(Repeat::One.as_str(), "one");
}

// --------------------------------------------------------------- engine

#[derive(Default)]
struct Fake {
    played: Vec<(PathBuf, u64)>,
    paused: bool,
    volume: f32,
    position_ms: u64,
    finished: bool,
    stops: usize,
}

struct FakeBackend(Arc<Mutex<Fake>>);

impl Backend for FakeBackend {
    fn play(&mut self, path: &Path, start_ms: u64) -> Result<(), PlayError> {
        if !path.exists() {
            return Err(PlayError::new("file_unavailable", "missing"));
        }
        let mut fake = self.0.lock().unwrap();
        fake.played.push((path.to_path_buf(), start_ms));
        fake.paused = false;
        fake.finished = false;
        fake.position_ms = start_ms;
        Ok(())
    }
    fn pause(&mut self) {
        self.0.lock().unwrap().paused = true;
    }
    fn resume(&mut self) {
        self.0.lock().unwrap().paused = false;
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().stops += 1;
    }
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
        self.0.lock().unwrap().finished
    }
}

fn engine() -> (Engine, Arc<Mutex<Fake>>) {
    let fake = Arc::new(Mutex::new(Fake::default()));
    (Engine::new(Box::new(FakeBackend(fake.clone())), 9), fake)
}

fn files(dir: &Path, count: i64) -> Vec<QueueItem> {
    (1..=count)
        .map(|id| {
            let path = dir.join(format!("{id}.flac"));
            std::fs::write(&path, b"x").unwrap();
            QueueItem {
                id,
                path,
                duration_ms: 200_000,
                meta: serde_json::json!({ "id": id }),
            }
        })
        .collect()
}

fn played_ids(fake: &Arc<Mutex<Fake>>) -> Vec<String> {
    fake.lock()
        .unwrap()
        .played
        .iter()
        .map(|(path, _)| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn engine_plays_first_item_then_next_and_previous() {
    let dir = tempdir().unwrap();
    let (mut engine, fake) = engine();
    engine.play_items(files(dir.path(), 3), 0).unwrap();
    assert!(engine.view().playing);
    assert_eq!(engine.view().track.unwrap()["id"], 1);

    engine.skip_next().unwrap();
    assert_eq!(engine.view().track.unwrap()["id"], 2);
    engine.skip_previous().unwrap();
    assert_eq!(engine.view().track.unwrap()["id"], 1);
    assert_eq!(played_ids(&fake), vec!["1", "2", "1"]);
}

#[test]
fn engine_previous_after_a_few_seconds_restarts_current_track() {
    let dir = tempdir().unwrap();
    let (mut engine, fake) = engine();
    engine.play_items(files(dir.path(), 3), 1).unwrap();
    fake.lock().unwrap().position_ms = 10_000;
    engine.skip_previous().unwrap();
    assert_eq!(engine.view().track.unwrap()["id"], 2);
    assert_eq!(fake.lock().unwrap().position_ms, 0);
    assert_eq!(played_ids(&fake), vec!["2"]);
}

#[test]
fn engine_advances_automatically_and_stops_after_the_last_track() {
    let dir = tempdir().unwrap();
    let (mut engine, fake) = engine();
    engine.play_items(files(dir.path(), 2), 0).unwrap();

    fake.lock().unwrap().finished = true;
    engine.tick();
    assert_eq!(engine.view().track.unwrap()["id"], 2);
    assert!(engine.view().playing);

    fake.lock().unwrap().finished = true;
    engine.tick();
    let view = engine.view();
    assert!(!view.playing);
    assert_eq!(view.position_ms, 0);
    assert_eq!(view.track.unwrap()["id"], 2);
    assert!(view.error_code.is_none());
}

#[test]
fn engine_repeat_all_loops_back_to_the_first_track() {
    let dir = tempdir().unwrap();
    let (mut engine, fake) = engine();
    engine.set_repeat("all").unwrap();
    engine.play_items(files(dir.path(), 2), 1).unwrap();
    fake.lock().unwrap().finished = true;
    engine.tick();
    assert_eq!(engine.view().track.unwrap()["id"], 1);
    assert_eq!(engine.view().repeat, "all");
}

#[test]
fn engine_missing_file_reports_file_unavailable_and_stops() {
    let dir = tempdir().unwrap();
    let (mut engine, _fake) = engine();
    let mut queue = files(dir.path(), 2);
    std::fs::remove_file(&queue[0].path).unwrap();
    let error = engine
        .play_items(std::mem::take(&mut queue), 0)
        .unwrap_err();
    assert_eq!(error.code, "file_unavailable");
    let view = engine.view();
    assert!(!view.playing);
    assert_eq!(view.error_code.as_deref(), Some("file_unavailable"));
}

#[test]
fn engine_next_skips_missing_files() {
    let dir = tempdir().unwrap();
    let (mut engine, fake) = engine();
    let queue = files(dir.path(), 3);
    std::fs::remove_file(&queue[1].path).unwrap();
    engine.play_items(queue, 0).unwrap();
    engine.skip_next().unwrap();
    assert_eq!(engine.view().track.unwrap()["id"], 3);
    assert!(engine.view().error_code.is_none());
    assert_eq!(played_ids(&fake), vec!["1", "3"]);
}

#[test]
fn engine_next_with_nothing_playable_left_stops_with_error() {
    let dir = tempdir().unwrap();
    let (mut engine, _fake) = engine();
    let queue = files(dir.path(), 3);
    std::fs::remove_file(&queue[1].path).unwrap();
    std::fs::remove_file(&queue[2].path).unwrap();
    engine.play_items(queue, 0).unwrap();
    let error = engine.skip_next().unwrap_err();
    assert_eq!(error.code, "file_unavailable");
    assert!(!engine.view().playing);
    assert_eq!(
        engine.view().error_code.as_deref(),
        Some("file_unavailable")
    );
}

#[test]
fn engine_pause_toggle_seek_and_volume() {
    let dir = tempdir().unwrap();
    let (mut engine, fake) = engine();
    engine.play_items(files(dir.path(), 1), 0).unwrap();

    fake.lock().unwrap().position_ms = 5_000;
    engine.pause();
    assert!(!engine.view().playing);
    assert!(fake.lock().unwrap().paused);
    assert_eq!(engine.view().position_ms, 5_000);

    engine.toggle().unwrap();
    assert!(engine.view().playing);
    assert!(!fake.lock().unwrap().paused);

    engine.seek(42_000).unwrap();
    assert_eq!(engine.view().position_ms, 42_000);

    engine.set_volume(3.0).unwrap();
    assert_eq!(engine.view().volume, 1.0);
    assert_eq!(fake.lock().unwrap().volume, 1.0);
    engine.set_volume(0.25).unwrap();
    assert_eq!(fake.lock().unwrap().volume, 0.25);
    assert!(engine.set_volume(f32::NAN).is_err());
    assert!(engine.set_repeat("sometimes").is_err());
}

#[test]
fn engine_restores_paused_track_and_resumes_from_saved_position() {
    let dir = tempdir().unwrap();
    let (mut engine, fake) = engine();
    engine.restore(files(dir.path(), 3), 1, 61_000);
    let view = engine.view();
    assert!(!view.playing);
    assert_eq!(view.position_ms, 61_000);
    assert_eq!(view.track.unwrap()["id"], 2);
    assert!(fake.lock().unwrap().played.is_empty());

    engine.toggle().unwrap();
    assert!(engine.view().playing);
    assert_eq!(fake.lock().unwrap().played[0].1, 61_000);
}

#[test]
fn playback_persistence_round_trips_and_ignores_garbage() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playback.json");
    assert_eq!(player::load_playback(&path), None);

    let saved = player::SavedPlayback {
        track_id: 12,
        position_ms: 34_500,
    };
    player::save_playback(&path, &saved).unwrap();
    assert_eq!(player::load_playback(&path), Some(saved));

    std::fs::write(&path, b"not json").unwrap();
    assert_eq!(player::load_playback(&path), None);
}
