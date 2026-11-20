use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, RwLock};

use crate::protocol::PlayerState;

#[derive(Clone)]
pub struct AppState {
    current: Arc<RwLock<PlayerState>>,
    changes: broadcast::Sender<PlayerState>,
    scan_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
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

    pub fn cancel_scan(&self) {
        if let Some(cancel) = self
            .scan_cancel
            .lock()
            .expect("scan cancellation lock was poisoned")
            .take()
        {
            cancel.store(true, Ordering::Relaxed);
        }
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
    }
}
