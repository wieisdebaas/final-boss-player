//! Streaming playback engine.
//!
//! Layers, bottom to top:
//! - [`Queue`]: pure logical play order (shuffle / repeat), no I/O.
//! - [`Backend`]: audio output. [`RodioBackend`] decodes straight from the open
//!   file through symphonia and never reads a whole file into memory; tests use
//!   a fake so no audio device is required.
//! - [`Engine`]: queue + backend state machine (play/pause/next/seek/...).
//! - [`Player`]: a dedicated thread that owns the engine (audio streams are not
//!   `Send`), mirrors it into [`AppState`] and persists the last position.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::scan::TimedIo;
use crate::state::{AppState, PlaybackUpdate};

/// `previous` restarts the current track when it is further in than this.
const RESTART_THRESHOLD_MS: u64 = 3_000;
const TICK: Duration = Duration::from_millis(100);
const PUBLISH_INTERVAL: Duration = Duration::from_millis(1_000);
const PERSIST_INTERVAL: Duration = Duration::from_secs(5);
/// How long a caller waits for the playback thread to answer a command before
/// reporting `player_busy` (the thread may be stuck on a stalled mount).
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest a single open + probe of an audio file may take.
const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
/// Threads that stalled opens may strand; further opens then fail fast.
const OPEN_WORKERS: usize = 2;

// ------------------------------------------------------------------ errors

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayError {
    pub code: String,
    pub message: String,
}

impl PlayError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    fn file_unavailable(path: &Path, detail: impl std::fmt::Display) -> Self {
        Self::new(
            "file_unavailable",
            format!("{} is unavailable: {detail}", path.display()),
        )
    }
}

impl std::fmt::Display for PlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PlayError {}

// ------------------------------------------------------------------- queue

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

impl Repeat {
    pub fn parse(mode: &str) -> Option<Self> {
        match mode {
            "off" => Some(Self::Off),
            "all" => Some(Self::All),
            "one" => Some(Self::One),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::All => "all",
            Self::One => "one",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub id: i64,
    pub path: PathBuf,
    pub duration_ms: u64,
    /// Track JSON exposed to clients as `PlayerState.track`.
    pub meta: Value,
}

/// Logical play order. `order` maps play position to an index in `items`.
pub struct Queue {
    items: Vec<QueueItem>,
    order: Vec<usize>,
    pos: usize,
    shuffle: bool,
    repeat: Repeat,
    rng: u64,
}

impl Queue {
    pub fn new(seed: u64) -> Self {
        Self {
            items: Vec::new(),
            order: Vec::new(),
            pos: 0,
            shuffle: false,
            repeat: Repeat::Off,
            rng: seed | 1,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Replaces the queue. Returns false (leaving it untouched) for an empty
    /// list or an out-of-range start index.
    pub fn set(&mut self, items: Vec<QueueItem>, start: usize) -> bool {
        if start >= items.len() {
            return false;
        }
        self.items = items;
        if self.shuffle {
            self.reshuffle(Some(start));
            self.pos = 0;
        } else {
            self.order = (0..self.items.len()).collect();
            self.pos = start;
        }
        true
    }

    pub fn current(&self) -> Option<&QueueItem> {
        self.order.get(self.pos).map(|&index| &self.items[index])
    }

    /// Advances. `auto` marks end-of-track advancement (repeat-one repeats).
    /// Returns `None` at the end of the queue, leaving the last track selected.
    pub fn next(&mut self, auto: bool) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }
        if auto && self.repeat == Repeat::One {
            return self.current();
        }
        if self.pos + 1 < self.order.len() {
            self.pos += 1;
        } else if self.repeat == Repeat::All {
            if self.shuffle {
                self.reshuffle(None);
            }
            self.pos = 0;
        } else {
            return None;
        }
        self.current()
    }

    /// Steps back; stays on the first track unless repeat-all wraps.
    pub fn previous(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }
        if self.pos > 0 {
            self.pos -= 1;
        } else if self.repeat == Repeat::All {
            self.pos = self.order.len() - 1;
        }
        self.current()
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        if enabled == self.shuffle {
            return;
        }
        self.shuffle = enabled;
        let current = self.order.get(self.pos).copied();
        if enabled {
            self.reshuffle(current);
            self.pos = 0;
        } else {
            self.order = (0..self.items.len()).collect();
            self.pos = current.unwrap_or(0);
        }
    }

    /// New random order, optionally pinning `first` to the front.
    fn reshuffle(&mut self, first: Option<usize>) {
        let mut order: Vec<usize> = (0..self.items.len())
            .filter(|index| Some(*index) != first)
            .collect();
        for i in (1..order.len()).rev() {
            let j = (self.next_random() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        if let Some(first) = first {
            order.insert(0, first);
        }
        self.order = order;
    }

    fn next_random(&mut self) -> u64 {
        // xorshift64*: plenty for shuffling a play queue.
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

// ----------------------------------------------------------------- backend

/// Audio output. `play` must stream from `path`, not read it whole.
pub trait Backend {
    fn play(&mut self, path: &Path, start_ms: u64) -> Result<(), PlayError>;
    fn pause(&mut self);
    fn resume(&mut self);
    fn stop(&mut self);
    fn seek(&mut self, position_ms: u64) -> Result<(), PlayError>;
    fn set_volume(&mut self, level: f32);
    fn position_ms(&self) -> u64;
    /// The loaded track has played to its end.
    fn finished(&self) -> bool;
}

/// rodio/cpal output with symphonia decoding (FLAC, MP3, Vorbis, AAC, WAV).
///
/// The output device is opened lazily on first play, so a daemon on a machine
/// without audio still serves the library and reports `audio_unavailable`
/// only when playback is actually requested.
pub struct RodioBackend {
    output: Option<(OutputStream, OutputStreamHandle)>,
    sink: Option<Sink>,
    volume: f32,
    base_ms: u64,
    /// Bounded worker pool for the file open / probe / initial seek, which can
    /// block indefinitely on a dead network mount.
    io: TimedIo,
}

impl RodioBackend {
    pub fn new() -> Self {
        Self::with_open_timeout(OPEN_TIMEOUT)
    }

    pub fn with_open_timeout(timeout: Duration) -> Self {
        Self {
            output: None,
            sink: None,
            volume: 1.0,
            base_ms: 0,
            io: TimedIo::new(OPEN_WORKERS, timeout),
        }
    }
}

type OpenedTrack = (Decoder<BufReader<File>>, u64);

/// Opens, probes and (optionally) seeks a file. Runs on a timed I/O worker.
fn open_decoder(path: &Path, start_ms: u64) -> Result<OpenedTrack, PlayError> {
    let file = File::open(path).map_err(|error| PlayError::file_unavailable(path, error))?;
    let mut decoder = Decoder::new(BufReader::new(file)).map_err(|error| {
        PlayError::new(
            "decode_failed",
            format!("could not decode {}: {error}", path.display()),
        )
    })?;
    let mut base_ms = 0;
    if start_ms > 0 {
        match decoder.try_seek(Duration::from_millis(start_ms)) {
            Ok(()) => base_ms = start_ms,
            Err(error) => tracing::warn!(%error, "could not seek to start position"),
        }
    }
    Ok((decoder, base_ms))
}

impl Default for RodioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for RodioBackend {
    fn play(&mut self, path: &Path, start_ms: u64) -> Result<(), PlayError> {
        // Open and probe before touching the current sink, so a bad file does
        // not interrupt whatever is playing.
        // The open runs on a timed worker: a stalled mount maps to
        // `file_unavailable` (so auto-advance can skip it) instead of wedging
        // the playback thread.
        let owned = path.to_path_buf();
        let (decoder, base_ms) = self
            .io
            .call(move || Ok(open_decoder(&owned, start_ms)))
            .map_err(|error| PlayError::file_unavailable(path, error))??;

        self.stop();
        if self.output.is_none() {
            self.output = Some(OutputStream::try_default().map_err(|error| {
                PlayError::new(
                    "audio_unavailable",
                    format!("no audio output device: {error}"),
                )
            })?);
        }
        let handle = &self.output.as_ref().expect("output was just opened").1;
        let sink = Sink::try_new(handle).map_err(|error| {
            PlayError::new("audio_unavailable", format!("audio output failed: {error}"))
        })?;
        sink.set_volume(self.volume);
        sink.append(decoder);
        self.sink = Some(sink);
        self.base_ms = base_ms;
        Ok(())
    }

    fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    fn resume(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.base_ms = 0;
    }

    fn seek(&mut self, position_ms: u64) -> Result<(), PlayError> {
        let sink = self
            .sink
            .as_ref()
            .ok_or_else(|| PlayError::new("not_playing", "nothing is loaded"))?;
        sink.try_seek(Duration::from_millis(position_ms))
            .map_err(|error| PlayError::new("seek_failed", error.to_string()))?;
        // Sink position is absolute again after an explicit seek.
        self.base_ms = 0;
        Ok(())
    }

    fn set_volume(&mut self, level: f32) {
        self.volume = level;
        if let Some(sink) = &self.sink {
            sink.set_volume(level);
        }
    }

    fn position_ms(&self) -> u64 {
        self.sink.as_ref().map_or(0, |sink| {
            self.base_ms + u64::try_from(sink.get_pos().as_millis()).unwrap_or(u64::MAX)
        })
    }

    fn finished(&self) -> bool {
        self.sink.as_ref().is_some_and(Sink::empty)
    }
}

// ------------------------------------------------------------------ engine

pub struct Engine {
    backend: Box<dyn Backend>,
    queue: Queue,
    /// A track is loaded in the backend (playing or paused).
    loaded: bool,
    playing: bool,
    /// Position to report/resume from while nothing is loaded.
    resume_ms: u64,
    volume: f32,
    error: Option<PlayError>,
    /// Where the current queue came from: "album", "artist", or "library".
    queue_scope: String,
    /// Level each track toward the same loudness.
    normalize: bool,
    /// Gain for the loaded track. `1.0` when normalization is off.
    track_gain: f32,
    gains: HashMap<String, f32>,
    settings_path: Option<PathBuf>,
}

impl Engine {
    pub fn new(backend: Box<dyn Backend>, seed: u64) -> Self {
        Self {
            backend,
            queue: Queue::new(seed),
            loaded: false,
            playing: false,
            resume_ms: 0,
            volume: 1.0,
            error: None,
            queue_scope: "album".into(),
            normalize: false,
            track_gain: 1.0,
            gains: HashMap::new(),
            settings_path: None,
        }
    }

    pub fn queue_scope(&self) -> &str {
        &self.queue_scope
    }

    pub fn set_queue_scope(&mut self, scope: &str) {
        self.queue_scope = if scope.is_empty() {
            "album".into()
        } else {
            scope.into()
        };
    }

    pub fn current_id(&self) -> Option<i64> {
        self.queue.current().map(|item| item.id)
    }

    pub fn view(&self) -> PlaybackUpdate {
        let current = self.queue.current();
        let position_ms = if self.loaded {
            self.backend.position_ms()
        } else {
            self.resume_ms
        };
        let duration_ms = current.map(|item| item.duration_ms).filter(|ms| *ms > 0);
        PlaybackUpdate {
            track: current.map(|item| item.meta.clone()),
            playing: self.playing,
            position_ms: duration_ms.map_or(position_ms, |duration| position_ms.min(duration)),
            duration_ms,
            volume: self.volume,
            shuffle: self.queue.shuffle(),
            repeat: self.queue.repeat().as_str().into(),
            error_code: self.error.as_ref().map(|error| error.code.clone()),
            error_message: self.error.as_ref().map(|error| error.message.clone()),
            scope: self.queue_scope.clone(),
            normalize: self.normalize,
        }
    }

    /// Replaces the queue and starts playing `items[start]`.
    pub fn play_items(&mut self, items: Vec<QueueItem>, start: usize) -> Result<(), PlayError> {
        if !self.queue.set(items, start) {
            return Err(PlayError::new("not_found", "nothing to play"));
        }
        self.start_current(0, Direction::Forward, false)
    }

    /// Loads a saved queue paused at `position_ms` without opening audio.
    pub fn restore(&mut self, items: Vec<QueueItem>, start: usize, position_ms: u64) {
        if self.queue.set(items, start) {
            self.backend.stop();
            self.loaded = false;
            self.playing = false;
            self.resume_ms = position_ms;
        }
    }

    pub fn pause(&mut self) {
        if self.loaded && self.playing {
            self.resume_ms = self.backend.position_ms();
            self.backend.pause();
            self.playing = false;
        }
    }

    pub fn toggle(&mut self) -> Result<(), PlayError> {
        if self.playing {
            self.pause();
            Ok(())
        } else if self.loaded {
            self.backend.resume();
            self.playing = true;
            Ok(())
        } else if self.queue.current().is_some() {
            let start = self.resume_ms;
            self.start_current(start, Direction::Forward, false)
        } else {
            Err(PlayError::new("not_found", "nothing to play"))
        }
    }

    pub fn skip_next(&mut self) -> Result<(), PlayError> {
        if self.queue.next(false).is_none() {
            // Past the last track: stop at the end rather than erroring.
            self.stop_at_end();
            return Ok(());
        }
        self.start_current(0, Direction::Forward, true)
    }

    pub fn skip_previous(&mut self) -> Result<(), PlayError> {
        if self.queue.current().is_none() {
            return Err(PlayError::new("not_found", "nothing to play"));
        }
        if self.loaded && self.backend.position_ms() > RESTART_THRESHOLD_MS {
            return self.seek(0);
        }
        self.queue.previous();
        self.start_current(0, Direction::Backward, true)
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<(), PlayError> {
        let duration = self.queue.current().map_or(0, |item| item.duration_ms);
        let position_ms = if duration > 0 {
            position_ms.min(duration)
        } else {
            position_ms
        };
        if self.loaded {
            self.backend.seek(position_ms)?;
        }
        self.resume_ms = position_ms;
        Ok(())
    }

    pub fn set_volume(&mut self, level: f32) -> Result<(), PlayError> {
        if !level.is_finite() {
            return Err(PlayError::new(
                "invalid_argument",
                "volume must be a number",
            ));
        }
        self.volume = level.clamp(0.0, 1.0);
        self.backend.set_volume(self.output_volume());
        Ok(())
    }

    pub fn set_normalize(&mut self, enabled: bool) {
        self.normalize = enabled;
        self.track_gain = self.current_track_gain();
        self.backend.set_volume(self.output_volume());
        self.save_settings();
    }

    /// Reads the saved Same volume switch. Missing files leave it off.
    pub fn load_settings(&mut self, path: &Path) {
        self.settings_path = Some(path.to_path_buf());
        let Ok(bytes) = fs::read(path) else {
            return;
        };
        #[derive(Deserialize)]
        struct Saved {
            #[serde(default)]
            normalize: bool,
        }
        if let Ok(saved) = serde_json::from_slice::<Saved>(&bytes) {
            self.normalize = saved.normalize;
        }
    }

    fn save_settings(&self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        let body = serde_json::json!({ "normalize": self.normalize });
        let Ok(bytes) = serde_json::to_vec(&body) else {
            return;
        };
        let temporary = path.with_extension("json.tmp");
        if fs::write(&temporary, bytes).is_ok() {
            let _ = fs::rename(temporary, path);
        }
    }

    fn output_volume(&self) -> f32 {
        (self.volume * self.track_gain).clamp(0.0, 4.0)
    }

    fn current_track_gain(&mut self) -> f32 {
        if !self.normalize {
            return 1.0;
        }
        let Some(path) = self.queue.current().map(|item| item.path.clone()) else {
            return 1.0;
        };
        self.gain_for(&path)
    }

    fn gain_for(&mut self, path: &Path) -> f32 {
        let key = match std::fs::metadata(path) {
            Ok(meta) => {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or(0);
                format!("{}:{}:{modified}", path.display(), meta.len())
            }
            Err(_) => return crate::loudness::playback_gain(path),
        };
        if let Some(gain) = self.gains.get(&key) {
            return *gain;
        }
        let gain = crate::loudness::playback_gain(path);
        self.gains.insert(key, gain);
        gain
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        self.queue.set_shuffle(enabled);
    }

    pub fn set_repeat(&mut self, mode: &str) -> Result<(), PlayError> {
        let repeat = Repeat::parse(mode).ok_or_else(|| {
            PlayError::new("invalid_argument", "repeat mode must be off, all or one")
        })?;
        self.queue.set_repeat(repeat);
        Ok(())
    }

    /// Call periodically: moves on when the current track has finished.
    pub fn tick(&mut self) {
        if !(self.loaded && self.playing && self.backend.finished()) {
            return;
        }
        if self.queue.next(true).is_some() {
            // A failure here is already recorded in `self.error`.
            let _ = self.start_current(0, Direction::Forward, true);
        } else {
            self.stop_at_end();
        }
    }

    fn stop_at_end(&mut self) {
        self.backend.stop();
        self.loaded = false;
        self.playing = false;
        self.resume_ms = 0;
    }

    /// Starts the selected track. With `skip_missing`, unavailable files are
    /// skipped in `direction`; otherwise the first failure stops playback.
    fn start_current(
        &mut self,
        start_ms: u64,
        direction: Direction,
        skip_missing: bool,
    ) -> Result<(), PlayError> {
        let mut start_ms = start_ms;
        let mut failure = None;
        for _ in 0..self.queue.len() {
            let Some(item) = self.queue.current() else {
                break;
            };
            let path = item.path.clone();
            self.track_gain = if self.normalize {
                self.gain_for(&path)
            } else {
                1.0
            };
            self.backend.set_volume(self.output_volume());
            match self.backend.play(&path, start_ms) {
                Ok(()) => {
                    self.loaded = true;
                    self.playing = true;
                    self.resume_ms = 0;
                    self.error = None;
                    return Ok(());
                }
                Err(error) if skip_missing && error.code == "file_unavailable" => {
                    tracing::warn!(%error, "skipping unavailable track");
                    failure = Some(error);
                    start_ms = 0;
                    let moved = match direction {
                        Direction::Forward => self.queue.next(false).is_some(),
                        Direction::Backward => {
                            let before = self.queue.current().map(|item| item.id);
                            self.queue.previous();
                            self.queue.current().map(|item| item.id) != before
                        }
                    };
                    if !moved {
                        break;
                    }
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }

        let error = failure.unwrap_or_else(|| PlayError::new("not_found", "nothing to play"));
        self.backend.stop();
        self.loaded = false;
        self.playing = false;
        self.resume_ms = 0;
        self.error = Some(error.clone());
        Err(error)
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
}

// ------------------------------------------------------------- persistence

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPlayback {
    pub track_id: i64,
    pub position_ms: u64,
    /// "album", "artist", or "library". Missing in older files, which means album.
    #[serde(default)]
    pub scope: String,
}

/// Atomically writes the last track and position.
pub fn save_playback(path: &Path, saved: &SavedPlayback) -> std::io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec(saved)?)?;
    fs::rename(temporary, path)
}

/// Missing or corrupt files simply mean "nothing to restore".
pub fn load_playback(path: &Path) -> Option<SavedPlayback> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

// ------------------------------------------------------------------ player

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    PlayItems {
        items: Vec<QueueItem>,
        start: usize,
        scope: String,
    },
    Pause,
    Toggle,
    Next,
    Previous,
    Seek(u64),
    SetVolume(f32),
    SetShuffle(bool),
    SetRepeat(String),
    SetNormalize(bool),
}

pub struct Restore {
    pub items: Vec<QueueItem>,
    pub start: usize,
    pub position_ms: u64,
    pub scope: String,
}

enum Request {
    Cmd(Cmd, oneshot::Sender<Result<(), PlayError>>),
}

/// Cheap, cloneable handle to the playback thread.
#[derive(Clone)]
pub struct Player {
    requests: mpsc::Sender<Request>,
}

impl Player {
    /// Spawns the playback thread. `make_backend` runs on that thread because
    /// audio output handles are not `Send`. The thread exits once every
    /// `Player` handle is dropped.
    pub fn spawn<F>(
        make_backend: F,
        state: AppState,
        runtime: tokio::runtime::Handle,
        persist_path: Option<PathBuf>,
        restore: Option<Restore>,
    ) -> Self
    where
        F: FnOnce() -> Box<dyn Backend> + Send + 'static,
    {
        let (requests, receiver) = mpsc::channel();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9, |elapsed| elapsed.as_nanos() as u64);
        let spawned = std::thread::Builder::new()
            .name("finalboss-player".into())
            .spawn(move || {
                let mut engine = Engine::new(make_backend(), seed);
                if let Some(path) = &persist_path {
                    engine.load_settings(&path.with_file_name("settings.json"));
                }
                if let Some(restore) = restore {
                    engine.set_queue_scope(&restore.scope);
                    engine.restore(restore.items, restore.start, restore.position_ms);
                }
                let mut sync = Mirror::new(state, runtime, persist_path);
                sync.publish(&engine, true);
                loop {
                    match receiver.recv_timeout(TICK) {
                        Ok(Request::Cmd(cmd, reply)) => {
                            let result = apply(&mut engine, cmd);
                            sync.publish(&engine, true);
                            let _ = reply.send(result);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            engine.tick();
                            sync.publish(&engine, false);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                engine.pause();
                sync.persist(&engine, true);
            });
        if let Err(error) = spawned {
            tracing::error!(%error, "could not start playback thread");
        }
        Self { requests }
    }

    /// Spawns the real audio engine.
    pub fn spawn_audio(
        state: AppState,
        runtime: tokio::runtime::Handle,
        persist_path: Option<PathBuf>,
        restore: Option<Restore>,
    ) -> Self {
        Self::spawn(
            || Box::new(RodioBackend::new()),
            state,
            runtime,
            persist_path,
            restore,
        )
    }

    /// Like [`Player::send`], but gives up with `player_busy` when the playback
    /// thread does not answer within [`REPLY_TIMEOUT`]. The command stays queued
    /// and still runs if the thread recovers.
    pub async fn send_timed(&self, cmd: Cmd) -> Result<(), PlayError> {
        self.send_timeout(cmd, REPLY_TIMEOUT).await
    }

    pub async fn send_timeout(&self, cmd: Cmd, timeout: Duration) -> Result<(), PlayError> {
        match tokio::time::timeout(timeout, self.send(cmd)).await {
            Ok(result) => result,
            Err(_) => Err(PlayError::new(
                "player_busy",
                "the player did not respond in time (is the music share unreachable?)",
            )),
        }
    }

    pub async fn send(&self, cmd: Cmd) -> Result<(), PlayError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Cmd(cmd, reply))
            .map_err(|_| PlayError::new("internal_error", "playback thread is not running"))?;
        response
            .await
            .map_err(|_| PlayError::new("internal_error", "playback thread stopped"))?
    }
}

fn apply(engine: &mut Engine, cmd: Cmd) -> Result<(), PlayError> {
    match cmd {
        Cmd::PlayItems {
            items,
            start,
            scope,
        } => {
            engine.set_queue_scope(&scope);
            engine.play_items(items, start)
        }
        Cmd::Pause => {
            engine.pause();
            Ok(())
        }
        Cmd::Toggle => engine.toggle(),
        Cmd::Next => engine.skip_next(),
        Cmd::Previous => engine.skip_previous(),
        Cmd::Seek(position_ms) => engine.seek(position_ms),
        Cmd::SetVolume(level) => engine.set_volume(level),
        Cmd::SetShuffle(enabled) => {
            engine.set_shuffle(enabled);
            Ok(())
        }
        Cmd::SetRepeat(mode) => engine.set_repeat(&mode),
        Cmd::SetNormalize(enabled) => {
            engine.set_normalize(enabled);
            Ok(())
        }
    }
}

/// Mirrors engine state into `AppState` and the on-disk playback file.
struct Mirror {
    state: AppState,
    runtime: tokio::runtime::Handle,
    persist_path: Option<PathBuf>,
    published: Option<PlaybackUpdate>,
    published_at: Instant,
    saved: Option<(i64, bool)>,
    saved_at: Instant,
}

impl Mirror {
    fn new(
        state: AppState,
        runtime: tokio::runtime::Handle,
        persist_path: Option<PathBuf>,
    ) -> Self {
        Self {
            state,
            runtime,
            persist_path,
            published: None,
            published_at: Instant::now(),
            saved: None,
            saved_at: Instant::now(),
        }
    }

    fn publish(&mut self, engine: &Engine, forced: bool) {
        let view = engine.view();
        let changed = self
            .published
            .as_ref()
            .is_none_or(|last| !last.same_except_position(&view));
        let position_due = view.playing && self.published_at.elapsed() >= PUBLISH_INTERVAL;
        if forced || changed || position_due {
            self.published = Some(view.clone());
            self.published_at = Instant::now();
            self.runtime.block_on(self.state.apply_playback(view));
        }
        self.persist(engine, false);
    }

    /// Persists on track/play-state changes, then every few seconds.
    fn persist(&mut self, engine: &Engine, force: bool) {
        let (Some(path), Some(track_id)) = (&self.persist_path, engine.current_id()) else {
            return;
        };
        let playing = engine.view().playing;
        let transition = self.saved != Some((track_id, playing));
        if !(force || transition || self.saved_at.elapsed() >= PERSIST_INTERVAL) {
            return;
        }
        let saved = SavedPlayback {
            track_id,
            position_ms: engine.view().position_ms,
            scope: engine.queue_scope().to_string(),
        };
        if let Err(error) = save_playback(path, &saved) {
            tracing::warn!(%error, "could not persist playback position");
        }
        self.saved = Some((track_id, playing));
        self.saved_at = Instant::now();
    }
}
