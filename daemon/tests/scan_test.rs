#[allow(dead_code)]
#[path = "../src/db.rs"]
mod db;
#[allow(dead_code)]
#[path = "../src/meta.rs"]
mod meta;
#[allow(dead_code)]
#[path = "../src/scan.rs"]
mod scan;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use db::Db;
use tempfile::tempdir;

#[test]
fn recognizes_supported_audio_extensions_case_insensitively() {
    for extension in ["flac", "mp3", "m4a", "aac", "wav", "ogg", "opus", "FLAC"] {
        assert!(scan::is_supported_audio(Path::new(&format!(
            "track.{extension}"
        ))));
    }

    for path in ["track.txt", "track.mp4", "track", ".mp3"] {
        assert!(!scan::is_supported_audio(Path::new(path)));
    }
}

#[test]
fn skips_only_unchanged_files_during_incremental_scans() {
    assert!(scan::should_skip_unchanged(
        Some((12_345, 1_700_000_000)),
        12_345,
        1_700_000_000,
        false,
    ));
    assert!(!scan::should_skip_unchanged(
        Some((12_346, 1_700_000_000)),
        12_345,
        1_700_000_000,
        false,
    ));
    assert!(!scan::should_skip_unchanged(
        Some((12_345, 1_700_000_001)),
        12_345,
        1_700_000_000,
        false,
    ));
    assert!(!scan::should_skip_unchanged(
        Some((12_345, 1_700_000_000)),
        12_345,
        1_700_000_000,
        true,
    ));
    assert!(!scan::should_skip_unchanged(
        None,
        12_345,
        1_700_000_000,
        false,
    ));
}

#[test]
fn scans_a_wave_file_into_sqlite_and_reports_progress() {
    let temp = tempdir().unwrap();
    let music = temp.path().join("music");
    std::fs::create_dir(&music).unwrap();
    write_wave(&music.join("Boss Theme.wav"));
    let db_path = temp.path().join("library.sqlite");
    let mut progress = Vec::new();

    let result = scan::run(
        Db::open(&db_path).unwrap(),
        &music,
        false,
        Arc::new(AtomicBool::new(false)),
        |update| progress.push(update),
    )
    .unwrap();

    let tracks = Db::open(db_path)
        .unwrap()
        .search("Boss Theme", 10, 0)
        .unwrap();
    assert_eq!(result.indexed, 1);
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Boss Theme");
    assert_eq!(progress.first().unwrap().scanned, 0);
    assert!(!progress.last().unwrap().scanning);
}

fn write_wave(path: &Path) {
    let data = vec![128_u8; 8_000];
    let mut wave = Vec::with_capacity(44 + data.len());
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36_u32 + data.len() as u32).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&8_000_u32.to_le_bytes());
    wave.extend_from_slice(&8_000_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&8_u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wave.extend_from_slice(&data);
    std::fs::write(path, wave).unwrap();
}
