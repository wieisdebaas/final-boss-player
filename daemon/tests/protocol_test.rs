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
fn play_artist_json_line_round_trips() {
    let line = r#"{"v":1,"id":12,"cmd":"play_artist","artist_id":7}"#;

    let message: ClientMessage = serde_json::from_str(line).unwrap();
    assert_eq!(
        message,
        ClientMessage {
            v: 1,
            id: 12,
            cmd: Command::PlayArtist { artist_id: 7 },
        }
    );
    assert_eq!(serde_json::to_string(&message).unwrap(), line);
}

#[test]
fn get_artist_art_json_line_round_trips() {
    let line = r#"{"v":1,"id":11,"cmd":"get_artist_art","artist_id":42}"#;

    let message: ClientMessage = serde_json::from_str(line).unwrap();
    assert_eq!(
        message,
        ClientMessage {
            v: 1,
            id: 11,
            cmd: Command::GetArtistArt { artist_id: 42 },
        }
    );
    assert_eq!(serde_json::to_string(&message).unwrap(), line);
}

#[test]
fn set_update_metadata_json_line_round_trips() {
    let line = r#"{"v":1,"id":13,"cmd":"set_update_metadata","enabled":true}"#;

    let message: ClientMessage = serde_json::from_str(line).unwrap();
    assert_eq!(
        message,
        ClientMessage {
            v: 1,
            id: 13,
            cmd: Command::SetUpdateMetadata { enabled: true },
        }
    );
    assert_eq!(serde_json::to_string(&message).unwrap(), line);
}

#[test]
fn play_without_ids_still_parses_and_search_ids_round_trip() {
    let album = r#"{"v":1,"id":4,"cmd":"play","track_id":9,"scope":"album"}"#;
    let parsed: ClientMessage = serde_json::from_str(album).unwrap();
    assert_eq!(
        parsed.cmd,
        Command::Play {
            track_id: 9,
            scope: "album".into(),
            track_ids: Vec::new(),
        }
    );

    let line = r#"{"v":1,"id":5,"cmd":"play","track_id":2,"scope":"search","track_ids":[2,8,1]}"#;
    let message: ClientMessage = serde_json::from_str(line).unwrap();
    assert_eq!(
        message,
        ClientMessage {
            v: 1,
            id: 5,
            cmd: Command::Play {
                track_id: 2,
                scope: "search".into(),
                track_ids: vec![2, 8, 1],
            },
        }
    );
    assert_eq!(serde_json::to_string(&message).unwrap(), line);
}

#[test]
fn play_genre_json_line_round_trips() {
    let line = r#"{"v":1,"id":14,"cmd":"play_genre","name":"Jazz"}"#;

    let message: ClientMessage = serde_json::from_str(line).unwrap();
    assert_eq!(
        message,
        ClientMessage {
            v: 1,
            id: 14,
            cmd: Command::PlayGenre {
                name: "Jazz".into()
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
