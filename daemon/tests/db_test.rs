#[allow(dead_code)]
#[path = "../src/db.rs"]
mod db;

use db::{Db, TrackInput};
use rusqlite::Connection;
use tempfile::tempdir;

fn track(folder_id: i64, path: &str, title: &str) -> TrackInput {
    TrackInput {
        folder_id,
        path: path.into(),
        size: 12_345,
        mtime: 1_700_000_000,
        title: title.into(),
        artist: Some("Final Form".into()),
        album_artist: Some("Final Form".into()),
        album: Some("Phase One".into()),
        track_no: Some(1),
        disc_no: Some(1),
        year: Some(2026),
        genre: Some("Game Soundtrack".into()),
        duration_ms: 180_000,
        has_art: true,
    }
}

#[test]
fn inserts_a_track_and_finds_it_by_title() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();

    let track_id = db
        .upsert_track(&track(folder_id, "/music/Boss Theme.flac", "Boss Theme"))
        .unwrap();

    let matches = db.search("boss theme", 20, 0).unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, track_id);
    assert_eq!(matches[0].title, "Boss Theme");
    assert_eq!(matches[0].artist.as_deref(), Some("Final Form"));
}

#[test]
fn creates_version_one_schema_and_lists_upserted_folders() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();

    let first_id = db.upsert_folder("/music", Some(100)).unwrap();
    let second_id = db.upsert_folder("/music", Some(200)).unwrap();
    let folders = db.list_folders().unwrap();

    assert_eq!(db.user_version().unwrap(), 1);
    assert_eq!(first_id, second_id);
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].path, "/music");
    assert_eq!(folders[0].last_scan_at, Some(200));
}

#[test]
fn exposes_album_artist_and_library_queries() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();
    let track_id = db
        .upsert_track(&track(folder_id, "/music/Boss Theme.flac", "Boss Theme"))
        .unwrap();

    let albums = db.list_albums(20, 0).unwrap();
    let artists = db.list_artists(20, 0).unwrap();
    let tracks = db.album_tracks(albums[0].id).unwrap();
    db.record_play(track_id, 300).unwrap();
    let stats = db.stats().unwrap();

    assert_eq!(albums[0].name, "Phase One");
    assert_eq!(albums[0].track_count, 1);
    assert_eq!(artists[0].name, "Final Form");
    assert_eq!(artists[0].track_count, 1);
    assert_eq!(tracks[0].id, track_id);
    assert_eq!(stats.folders, 1);
    assert_eq!(stats.tracks, 1);
    assert_eq!(stats.albums, 1);
    assert_eq!(stats.artists, 1);
    assert_eq!(stats.plays, 1);
}

#[test]
fn removes_tracks_missing_from_a_folder_scan() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();
    db.upsert_track(&track(folder_id, "/music/keep.flac", "Keep"))
        .unwrap();
    db.upsert_track(&track(folder_id, "/music/remove.flac", "Remove"))
        .unwrap();

    let removed = db
        .remove_missing_under(folder_id, &["/music/keep.flac".into()])
        .unwrap();

    assert_eq!(removed, 1);
    assert_eq!(db.search("", 20, 0).unwrap().len(), 1);
    assert_eq!(db.search("remove", 20, 0).unwrap().len(), 0);
}

#[test]
fn rejects_newer_schema_without_downgrading_it() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA user_version = 2;")
        .unwrap();
    drop(connection);

    assert!(Db::open(&path).is_err());

    let connection = Connection::open(path).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 2);
    assert_eq!(table_count, 0);
}

#[test]
fn failed_track_upsert_rolls_back_artist_and_album() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();

    assert!(db
        .upsert_track(&track(999, "/music/missing.flac", "Missing"))
        .is_err());

    let stats = db.stats().unwrap();
    assert_eq!(stats.tracks, 0);
    assert_eq!(stats.albums, 0);
    assert_eq!(stats.artists, 0);
}

#[test]
fn failed_remove_missing_rolls_back_all_deletes() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    {
        let db = Db::open(&path).unwrap();
        let folder_id = db.upsert_folder("/music", None).unwrap();
        db.upsert_track(&track(folder_id, "/music/a.flac", "A"))
            .unwrap();
        db.upsert_track(&track(folder_id, "/music/b.flac", "B"))
            .unwrap();
    }
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_second_delete
             BEFORE DELETE ON tracks
             WHEN OLD.path = '/music/b.flac'
             BEGIN
                 SELECT RAISE(ABORT, 'forced delete failure');
             END;",
        )
        .unwrap();
    drop(connection);
    let db = Db::open(path).unwrap();
    let folder_id = db.list_folders().unwrap()[0].id;

    assert!(db.remove_missing_under(folder_id, &[]).is_err());
    assert_eq!(db.search("", 20, 0).unwrap().len(), 2);
}

#[test]
fn ensure_folder_is_idempotent_and_keeps_scan_time() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();

    assert_eq!(db.folder_id("/music").unwrap(), None);
    let first = db.ensure_folder("/music").unwrap();
    db.mark_folder_scanned(first, 500).unwrap();
    let second = db.ensure_folder("/music").unwrap();

    assert_eq!(first, second);
    assert_eq!(db.folder_id("/music").unwrap(), Some(first));
    assert_eq!(db.list_folders().unwrap()[0].last_scan_at, Some(500));
}

#[test]
fn concurrent_first_opens_all_migrate_safely() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                Db::open(&path).map(|db| db.user_version().unwrap())
            })
        })
        .collect();

    for handle in handles {
        assert_eq!(handle.join().unwrap().unwrap(), 1);
    }
}

#[test]
fn open_enables_wal_journal_mode() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    drop(Db::open(&path).unwrap());

    let connection = Connection::open(path).unwrap();
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}
