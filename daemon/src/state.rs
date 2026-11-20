use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};

use crate::protocol::PlayerState;

#[derive(Clone)]
pub struct AppState {
    current: Arc<RwLock<PlayerState>>,
    changes: broadcast::Sender<PlayerState>,
}

impl AppState {
    pub fn new() -> Self {
        let (changes, _) = broadcast::channel(16);
        Self {
            current: Arc::new(RwLock::new(initial_player_state())),
            changes,
        }
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
