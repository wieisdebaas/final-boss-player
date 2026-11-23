use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, RwLock};

use crate::protocol::PlayerState;

#[derive(Clone)]
pub struct AppState {
    current: Arc<RwLock<PlayerState>>,
    changes: broadcast::Sender<PlayerState>,
    /// Cancel flag of the newest scan task that has not finished yet.
    scan_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    /// A full rescan was requested and not yet carried out to completion. It
    /// survives cancellation so a superseding scan inherits it.
    full_scan_pending: Arc<AtomicBool>,
    /// Held for the whole lifetime of a scan task and by folder removal, so
    /// `remove_folder` can wait for a cancelled scan to stop writing.
    scan_gate: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub fn new() -> Self {
        let (changes, _) = broadcast::channel(16);
        Self {
            current: Arc::new(RwLock::new(initial_player_state())),
            changes,
            scan_cancel: Arc::new(Mutex::new(None)),
            full_scan_pending: Arc::new(AtomicBool::new(false)),
            scan_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub async fn acquire_scan_gate(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.scan_gate.clone().lock_owned().await
    }

    pub async fn snapshot(&self) -> PlayerState {
        self.current.read().await.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PlayerState> {
        self.changes.subscribe()
    }

    pub async fn replace(&self, state: PlayerState) {
        *self.current.write().await = state.clone();
        let _ = self.changes.send(state);
    }

    pub fn begin_scan(&self) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut active = self
            .scan_cancel
            .lock()
            .expect("scan cancellation lock was poisoned");
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
        cancel
    }

    /// Cancels the active scan, if any. Returns whether one was cancelled.
    pub fn cancel_scan(&self) -> bool {
        match self
            .scan_cancel
            .lock()
            .expect("scan cancellation lock was poisoned")
            .take()
        {
            Some(cancel) => {
                cancel.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Marks a scan task as finished (no-op if a newer scan already replaced it).
    pub fn end_scan(&self, cancel: &Arc<AtomicBool>) {
        let mut active = self
            .scan_cancel
            .lock()
            .expect("scan cancellation lock was poisoned");
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, cancel))
        {
            *active = None;
        }
    }

    pub fn request_full_scan(&self) {
        self.full_scan_pending.store(true, Ordering::SeqCst);
    }

    /// Takes the pending full-scan request; the caller must re-request it if
    /// it gets cancelled before finishing.
    pub fn take_full_scan(&self) -> bool {
        self.full_scan_pending.swap(false, Ordering::SeqCst)
    }

    pub async fn update_scan(&self, progress: crate::scan::ScanProgress) {
        let mut snapshot = self.current.write().await;
        snapshot.scan =
            Some(serde_json::to_value(progress).expect("scan progress is serializable"));
        snapshot.error_code = None;
        snapshot.error_message = None;
        snapshot.generation += 1;
        let _ = self.changes.send(snapshot.clone());
    }

    pub async fn finish_scan(&self) {
        let mut snapshot = self.current.write().await;
        if let Some(scan) = snapshot
            .scan
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        {
            scan.insert("scanning".into(), false.into());
        }
        snapshot.generation += 1;
        let _ = self.changes.send(snapshot.clone());
    }

    pub async fn set_folders(&self, folders: Vec<String>) {
        let mut snapshot = self.current.write().await;
        snapshot.folders = folders;
        snapshot.generation += 1;
        let _ = self.changes.send(snapshot.clone());
    }

    pub async fn set_scan_error(&self, message: String) {
        let mut snapshot = self.current.write().await;
        snapshot.scan = None;
        snapshot.error_code = Some("scan_failed".into());
        snapshot.error_message = Some(message);
        snapshot.generation += 1;
        let _ = self.changes.send(snapshot.clone());
    }
}

/// Playback fields the player engine owns inside the shared snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackUpdate {
    pub track: Option<serde_json::Value>,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub scope: String,
    pub normalize: bool,
}

impl PlaybackUpdate {
    /// Equality that ignores the (continuously advancing) position.
    pub fn same_except_position(&self, other: &Self) -> bool {
        let mut other = other.clone();
        other.position_ms = self.position_ms;
        *self == other
    }
}

impl AppState {
    pub async fn apply_playback(&self, update: PlaybackUpdate) {
        let mut snapshot = self.current.write().await;
        snapshot.track = update.track;
        snapshot.playing = update.playing;
        snapshot.position_ms = update.position_ms;
        snapshot.duration_ms = update.duration_ms;
        snapshot.volume = update.volume;
        snapshot.shuffle = update.shuffle;
        snapshot.repeat = update.repeat;
        snapshot.scope = update.scope;
        snapshot.normalize = update.normalize;
        // Playback must not clear a scan failure, but owns its own errors.
        if update.error_code.is_some() || snapshot.error_code.as_deref() != Some("scan_failed") {
            snapshot.error_code = update.error_code;
            snapshot.error_message = update.error_message;
        }
        snapshot.generation += 1;
        let _ = self.changes.send(snapshot.clone());
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn initial_player_state() -> PlayerState {
    PlayerState {
        lifecycle: "ready".into(),
        playing: false,
        track: None,
        position_ms: 0,
        duration_ms: None,
        volume: 1.0,
        shuffle: false,
        repeat: "off".into(),
        scan: None,
        folders: Vec::new(),
        error_code: None,
        error_message: None,
        generation: 0,
        scope: "album".into(),
        normalize: false,
    }
}
