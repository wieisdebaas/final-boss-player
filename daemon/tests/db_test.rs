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
fn lists_recently_added_tracks_newest_first() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    let db = Db::open(&path).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();
    db.upsert_track(&track(folder_id, "/music/old.flac", "Old"))
        .unwrap();
    db.upsert_track(&track(folder_id, "/music/new.flac", "New"))
        .unwrap();
    db.upsert_track(&track(folder_id, "/music/also-new.flac", "Also New"))
        .unwrap();
    drop(db);

    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE tracks SET added_at = 10 WHERE title = 'Old'", [])
        .unwrap();
    connection
        .execute("UPDATE tracks SET added_at = 30 WHERE title = 'New'", [])
        .unwrap();
    connection
        .execute(
            "UPDATE tracks SET added_at = 30 WHERE title = 'Also New'",
            [],
        )
        .unwrap();
    drop(connection);

    let db = Db::open(&path).unwrap();
    let recent = db.list_recent_tracks(2, 0).unwrap();

    assert_eq!(
        recent
            .iter()
            .map(|track| track.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Also New", "New"]
    );
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

    let mut marshall = track(
        folder_id,
        "/music/real-slim-shady.flac",
        "The Real Slim Shady",
    );
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

    assert_eq!(db.user_version().unwrap(), 2);
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
    assert!(artists[0].has_art);
    assert_eq!(
        db.artist_art_source(artists[0].id).unwrap().as_deref(),
        Some("/music/Boss Theme.flac")
    );
    assert_eq!(tracks[0].id, track_id);
    assert_eq!(stats.folders, 1);
    assert_eq!(stats.tracks, 1);
    assert_eq!(stats.albums, 1);
    assert_eq!(stats.artists, 1);
    assert_eq!(stats.plays, 1);
}

#[test]
fn artist_art_skips_tracks_without_a_picture() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();

    let mut bare = track(folder_id, "/music/bare.flac", "Bare");
    bare.has_art = false;
    bare.track_no = Some(1);
    db.upsert_track(&bare).unwrap();

    let mut pictured = track(folder_id, "/music/pictured.flac", "Pictured");
    pictured.track_no = Some(2);
    db.upsert_track(&pictured).unwrap();

    let artist = &db.list_artists(10, 0).unwrap()[0];
    assert!(artist.has_art);
    assert_eq!(
        db.artist_art_source(artist.id).unwrap().as_deref(),
        Some("/music/pictured.flac")
    );

    let mut silent = track(folder_id, "/music/silent.flac", "Silent");
    silent.artist = Some("No Cover".into());
    silent.album_artist = Some("No Cover".into());
    silent.album = Some("Empty".into());
    silent.has_art = false;
    db.upsert_track(&silent).unwrap();
    let no_cover = db
        .list_artists(10, 0)
        .unwrap()
        .into_iter()
        .find(|artist| artist.name == "No Cover")
        .unwrap();
    assert!(!no_cover.has_art);
    assert_eq!(db.artist_art_source(no_cover.id).unwrap(), None);
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
        .execute_batch("PRAGMA user_version = 3;")
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
    assert_eq!(version, 3);
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
        assert_eq!(handle.join().unwrap().unwrap(), 2);
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

#[test]
fn genres_group_tracks_and_a_lookup_is_remembered() {
    let temp = tempdir().unwrap();
    let db = Db::open(temp.path().join("library.sqlite")).unwrap();
    let folder_id = db.upsert_folder("/music", None).unwrap();

    let mut jazz = scoped(folder_id, "/music/j1.flac", "Blue", "Ada", "Cool", 1);
    jazz.genre = Some("Jazz".into());
    jazz.has_art = true;
    let jazz_id = db.upsert_track(&jazz).unwrap();

    let mut jazz_two = scoped(folder_id, "/music/j2.flac", "Bluer", "Ada", "Cool", 2);
    jazz_two.genre = Some("jazz".into());
    jazz_two.has_art = false;
    db.upsert_track(&jazz_two).unwrap();

    let mut bare = scoped(folder_id, "/music/bare.flac", "Bare", "Bea", "Empty", 1);
    bare.genre = None;
    bare.has_art = false;
    let bare_id = db.upsert_track(&bare).unwrap();

    let genres = db.list_genres(20, 0).unwrap();
    assert_eq!(genres.len(), 1);
    assert_eq!(genres[0].name.to_lowercase(), "jazz");
    assert_eq!(genres[0].track_count, 2);
    assert!(genres[0].has_art);

    let tracks = db.genre_tracks("JAZZ", 20, 0).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(
        titles(db.queue_for_scope(jazz_id, "genre").unwrap()).0,
        vec!["Blue".to_string(), "Bluer".to_string()]
    );

    let gaps = db.albums_missing_metadata().unwrap();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].name, "Empty");
    assert!(gaps[0].needs_genre);
    assert!(gaps[0].needs_art);

    let album_id = db
        .list_albums(20, 0)
        .unwrap()
        .into_iter()
        .find(|album| album.name == "Empty")
        .unwrap()
        .id;
    assert_eq!(db.fill_empty_genre(album_id, "Rock").unwrap(), 1);
    assert_eq!(db.genre_tracks("Rock", 20, 0).unwrap()[0].id, bare_id);
    // An existing genre is left alone.
    assert_eq!(db.fill_empty_genre(album_id, "Metal").unwrap(), 0);

    db.mark_album_cover(album_id).unwrap();
    let empty = db
        .list_albums(20, 0)
        .unwrap()
        .into_iter()
        .find(|album| album.name == "Empty")
        .unwrap();
    assert!(empty.has_art);

    let key = db::album_key("Bea", "Empty");
    db.remember_lookup(&key, true, Some("mbid"), Some("Rock"))
        .unwrap();
    assert!(db.albums_missing_metadata().unwrap().is_empty());

    // A rescan that clears the tag is filled back from the remembered lookup.
    let mut cleared = bare.clone();
    cleared.genre = None;
    db.upsert_track(&cleared).unwrap();
    assert_eq!(db.refill_cached_genres().unwrap(), 1);
    assert_eq!(
        db.genre_tracks("Rock", 20, 0).unwrap()[0].genre.as_deref(),
        Some("Rock")
    );
}

#[test]
fn migrates_a_version_one_database() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("library.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE albums (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                album_artist TEXT NOT NULL DEFAULT '',
                year INTEGER
             );
             PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(connection);

    let db = Db::open(&path).unwrap();
    assert_eq!(db.user_version().unwrap(), 2);
    let connection = Connection::open(&path).unwrap();
    let has_cover: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('albums') WHERE name = 'has_cover'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let lookups: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'metadata_lookups'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(has_cover, 1);
    assert_eq!(lookups, 1);
}
