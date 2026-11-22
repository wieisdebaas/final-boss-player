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
fn comma_separated_terms_must_all_match() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();

    let mut shady = track(folder_id, "/music/my-name-is.flac", "My Name Is");
    shady.artist = Some("Eminem".into());
    shady.album_artist = Some("Eminem".into());
    shady.album = Some("The Slim Shady LP".into());
    db.upsert_track(&shady).unwrap();

    let mut marshall = track(folder_id, "/music/real-slim-shady.flac", "The Real Slim Shady");
    marshall.artist = Some("Eminem".into());
    marshall.album_artist = Some("Eminem".into());
    marshall.album = Some("The Marshall Mathers LP".into());
    db.upsert_track(&marshall).unwrap();

    let mut other = track(folder_id, "/music/cover.flac", "My Name Is");
    other.artist = Some("Someone Else".into());
    other.album_artist = Some("Someone Else".into());
    other.album = Some("The Slim Shady LP".into());
    db.upsert_track(&other).unwrap();

    let both = db.search("eminem, the slim shady lp", 20, 0).unwrap();
    assert_eq!(both.len(), 1);
    assert_eq!(both[0].title, "My Name Is");
    assert_eq!(both[0].artist.as_deref(), Some("Eminem"));

    assert_eq!(db.search("eminem", 20, 0).unwrap().len(), 2);
    assert_eq!(db.search("eminem,", 20, 0).unwrap().len(), 2);
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
fn collapses_slash_artists_onto_the_primary_name() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();
    let mut collab = track(folder_id, "/music/questions.flac", "21 Questions");
    collab.artist = Some("50 Cent/Nate Dogg".into());
    let mut solo = track(folder_id, "/music/club.flac", "In Da Club");
    solo.artist = Some("50 Cent".into());
    db.upsert_track(&collab).unwrap();
    db.upsert_track(&solo).unwrap();

    let changed = db
        .rewrite_collaborations(|title, artist| {
            let (primary, guest) = artist.split_once('/').unwrap();
            (format!("{title} - featuring {guest}"), primary.to_owned())
        })
        .unwrap();

    assert_eq!(changed, 1);
    let artists = db.list_artists(10, 0).unwrap();
    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].name, "50 Cent");
    assert_eq!(artists[0].track_count, 2);
    let questions = db
        .list_tracks(10, 0)
        .unwrap()
        .into_iter()
        .find(|track| track.path.ends_with("questions.flac"))
        .unwrap();
    assert_eq!(questions.title, "21 Questions - featuring Nate Dogg");
    assert_eq!(questions.artist.as_deref(), Some("50 Cent"));
    assert_eq!(
        db.rewrite_collaborations(|title, artist| (title.to_owned(), artist.to_owned()))
            .unwrap(),
        0
    );
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

fn scoped(
    folder_id: i64,
    path: &str,
    title: &str,
    artist: &str,
    album: &str,
    track_no: i64,
) -> TrackInput {
    let mut input = track(folder_id, path, title);
    input.artist = Some(artist.into());
    input.album_artist = Some(artist.into());
    input.album = Some(album.into());
    input.track_no = Some(track_no);
    input
}

fn titles(queue: Option<(Vec<db::Track>, usize)>) -> (Vec<String>, usize) {
    let (tracks, index) = queue.expect("queue");
    (tracks.into_iter().map(|track| track.title).collect(), index)
}

#[test]
fn play_queue_follows_album_artist_or_the_whole_library() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();
    let ada_one = db
        .upsert_track(&scoped(
            folder_id,
            "/music/a1.flac",
            "Ada One",
            "Ada",
            "Alpha",
            1,
        ))
        .unwrap();
    let ada_two = db
        .upsert_track(&scoped(
            folder_id,
            "/music/a2.flac",
            "Ada Two",
            "Ada",
            "Alpha",
            2,
        ))
        .unwrap();
    let ada_beta = db
        .upsert_track(&scoped(
            folder_id,
            "/music/b1.flac",
            "Ada Beta",
            "Ada",
            "Beta",
            1,
        ))
        .unwrap();
    let _bea = db
        .upsert_track(&scoped(
            folder_id,
            "/music/c1.flac",
            "Bea One",
            "Bea",
            "Gamma",
            1,
        ))
        .unwrap();

    assert_eq!(
        titles(db.queue_for_scope(ada_two, "album").unwrap()),
        (vec!["Ada One".into(), "Ada Two".into()], 1)
    );
    assert_eq!(
        titles(db.queue_for_scope(ada_beta, "artist").unwrap()),
        (
            vec!["Ada One".into(), "Ada Two".into(), "Ada Beta".into()],
            2
        )
    );
    assert_eq!(
        titles(db.queue_for_scope(ada_one, "library").unwrap()),
        (
            vec![
                "Ada Beta".into(),
                "Ada One".into(),
                "Ada Two".into(),
                "Bea One".into(),
            ],
            1
        )
    );
    assert!(db.queue_for_scope(9999, "library").unwrap().is_none());
}
