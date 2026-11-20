#[path = "../src/protocol.rs"]
pub mod protocol;

use protocol::{parse_client_message, ClientMessage, Command};

#[test]
fn pause_json_line_round_trips() {
    let line = r#"{"v":1,"id":7,"cmd":"pause"}"#;

    let message: ClientMessage = serde_json::from_str(line).unwrap();
    assert_eq!(
        message,
        ClientMessage {
            v: 1,
            id: 7,
            cmd: Command::Pause,
        }
    );
    assert_eq!(serde_json::to_string(&message).unwrap(), line);
}

#[test]
fn seek_json_line_round_trips() {
    let line = r#"{"v":1,"id":8,"cmd":"seek","position_ms":42000}"#;

    let message: ClientMessage = serde_json::from_str(line).unwrap();
    assert_eq!(
        message,
        ClientMessage {
            v: 1,
            id: 8,
            cmd: Command::Seek {
                position_ms: 42_000
            },
        }
    );
    assert_eq!(serde_json::to_string(&message).unwrap(), line);
}

#[test]
fn unsupported_protocol_version_has_stable_error_code() {
    let error = parse_client_message(r#"{"v":2,"id":9,"cmd":"ping"}"#).unwrap_err();

    assert_eq!(error.code, "unsupported_version");
}
