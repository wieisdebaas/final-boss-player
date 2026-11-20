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
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use db::{Db, TrackInput};
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

#[test]
fn cancelled_scan_does_not_recreate_folder_or_touch_db() {
    let temp = tempdir().unwrap();
    let music = temp.path().join("music");
    std::fs::create_dir(&music).unwrap();
    write_wave(&music.join("Boss Theme.wav"));
    let db_path = temp.path().join("library.sqlite");

    let result = scan::run(
        Db::open(&db_path).unwrap(),
        &music,
        false,
        Arc::new(AtomicBool::new(true)),
        |_| {},
    )
    .unwrap();

    assert!(result.cancelled);
    let db = Db::open(db_path).unwrap();
    assert!(db.list_folders().unwrap().is_empty());
    assert_eq!(db.stats().unwrap().tracks, 0);
}

#[test]
fn incomplete_traversal_never_prunes_indexed_tracks() {
    let temp = tempdir().unwrap();
    let music = temp.path().join("music");
    let flaky = music.join("flaky");
    std::fs::create_dir_all(&flaky).unwrap();
    write_wave(&music.join("Kept Root.wav"));
    write_wave(&flaky.join("Kept Nested.wav"));
    let db_path = temp.path().join("library.sqlite");
    let io = scan::TimedIo::new(2, Duration::from_millis(200));

    // First, a complete scan indexes both files.
    let first = scan::run_with(
        Db::open(&db_path).unwrap(),
        &music,
        false,
        Arc::new(AtomicBool::new(false)),
        &io,
        Arc::new(scan::read_dir_entries),
        |_| {},
    )
    .unwrap();
    assert!(first.discovery_complete);
    assert_eq!(first.indexed, 2);

    // Then the "flaky" subtree fails to list: its tracks must survive.
    let failing_dir = flaky.clone();
    let result = scan::run_with(
        Db::open(&db_path).unwrap(),
        &music,
        true,
        Arc::new(AtomicBool::new(false)),
        &io,
        Arc::new(move |dir: &Path| {
            if dir == failing_dir {
                Err(std::io::Error::other("stale NAS handle"))
            } else {
                scan::read_dir_entries(dir)
            }
        }),
        |_| {},
    )
    .unwrap();

    assert!(!result.discovery_complete);
    assert_eq!(result.removed, 0);
    let db = Db::open(&db_path).unwrap();
    assert_eq!(db.stats().unwrap().tracks, 2);

    // A later complete scan still prunes files that are really gone.
    std::fs::remove_file(flaky.join("Kept Nested.wav")).unwrap();
    let healed = scan::run_with(
        Db::open(&db_path).unwrap(),
        &music,
        false,
        Arc::new(AtomicBool::new(false)),
        &io,
        Arc::new(scan::read_dir_entries),
        |_| {},
    )
    .unwrap();
    assert!(healed.discovery_complete);
    assert_eq!(healed.removed, 1);
    assert_eq!(db.stats().unwrap().tracks, 1);
}

#[test]
fn stalled_directory_listing_times_out_without_hanging_or_pruning() {
    let temp = tempdir().unwrap();
    let music = temp.path().join("music");
    let stalled = music.join("stalled");
    std::fs::create_dir_all(&stalled).unwrap();
    write_wave(&music.join("Root.wav"));
    let db_path = temp.path().join("library.sqlite");
    let db = Db::open(&db_path).unwrap();
    let folder_id = db.upsert_folder(&music.to_string_lossy(), None).unwrap();
    let stalled_track = stalled.join("Hidden.wav").to_string_lossy().into_owned();
    db.upsert_track(&TrackInput {
        folder_id,
        path: stalled_track,
        size: 1,
        mtime: 1,
        title: "Hidden".into(),
        artist: None,
        album_artist: None,
        album: None,
        track_no: None,
        disc_no: None,
        year: None,
        genre: None,
        duration_ms: 1,
        has_art: false,
    })
    .unwrap();

    let (release, blocked) = mpsc::channel::<()>();
    let blocked = Arc::new(Mutex::new(blocked));
    let stalled_dir = stalled.clone();
    let io = scan::TimedIo::new(2, Duration::from_millis(100));
    let started = Instant::now();
    let result = scan::run_with(
        Db::open(&db_path).unwrap(),
        &music,
        false,
        Arc::new(AtomicBool::new(false)),
        &io,
        Arc::new(move |dir: &Path| {
            if dir == stalled_dir {
                let _ = blocked.lock().unwrap().recv();
                return Ok(Vec::new());
            }
            scan::read_dir_entries(dir)
        }),
        |_| {},
    )
    .unwrap();

    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!result.discovery_complete);
    assert_eq!(result.removed, 0);
    assert_eq!(db.stats().unwrap().tracks, 2);
    drop(release);
}

#[test]
fn timed_io_is_bounded_and_recovers_when_stalled_work_finishes() {
    let io = scan::TimedIo::new(1, Duration::from_millis(50));
    let (release, blocked) = mpsc::channel::<()>();

    let first = io.call(move || {
        let _ = blocked.recv();
        Ok(())
    });
    assert!(first.unwrap_err().to_string().contains("timed out"));
    assert_eq!(io.in_flight(), 1);

    // The only worker is stuck: further calls fail fast and spawn nothing.
    let started = Instant::now();
    let second = io.call(|| Ok(1));
    assert!(second.unwrap_err().to_string().contains("unavailable"));
    assert!(started.elapsed() < Duration::from_millis(40));
    assert_eq!(io.in_flight(), 1);

    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while io.in_flight() != 0 {
        assert!(Instant::now() < deadline, "permit was never released");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(io.call(|| Ok(7)).unwrap(), 7);
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
