use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientMessage {
    pub v: u32,
    pub id: u64,
    #[serde(flatten)]
    pub cmd: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Hello,
    Ping,
    GetState,
    AddFolder {
        path: String,
    },
    RemoveFolder {
        path: String,
    },
    Rescan {
        full: bool,
    },
    Query {
        kind: String,
        #[serde(default)]
        q: String,
        #[serde(default = "default_query_limit")]
        limit: u32,
        #[serde(default)]
        offset: u32,
    },
    Play {
        track_id: i64,
    },
    PlayAlbum {
        album_id: i64,
    },
    Pause,
    Toggle,
    Next,
    Previous,
    Seek {
        position_ms: u64,
    },
    SetVolume {
        level: f32,
    },
    SetShuffle {
        enabled: bool,
    },
    SetRepeat {
        mode: String,
    },
    GetArt {
        album_id: i64,
    },
}

fn default_query_limit() -> u32 {
    100
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Response {
        id: u64,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ProtocolError>,
    },
    Event {
        event: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        state: Option<PlayerState>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub lifecycle: String,
    pub playing: bool,
    pub track: Option<Value>,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: String,
    pub scan: Option<Value>,
    pub folders: Vec<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

impl ProtocolError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Parses one UTF-8 JSON line and enforces protocol version 1.
pub fn parse_client_message(line: &str) -> Result<ClientMessage, ProtocolError> {
    let message: ClientMessage = serde_json::from_str(line)
        .map_err(|error| ProtocolError::new("invalid_request", error.to_string()))?;

    if message.v != PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            "unsupported_version",
            format!("protocol version {} is unsupported", message.v),
        ));
    }

    Ok(message)
}
