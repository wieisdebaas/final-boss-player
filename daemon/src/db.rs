use std::collections::HashSet;
use std::path::Path;

use rusqlite::{ffi, params, params_from_iter, Connection, Result, TransactionBehavior};
use serde::Serialize;

const SUPPORTED_SCHEMA_VERSION: i64 = 1;
const VERSION_ONE_SCHEMA: &str = "
    CREATE TABLE folders (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        last_scan_at INTEGER
    );
    CREATE TABLE artists (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE
    );
    CREATE TABLE albums (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        album_artist TEXT NOT NULL DEFAULT '',
        year INTEGER,
        UNIQUE(name, album_artist)
    );
    CREATE TABLE tracks (
        id INTEGER PRIMARY KEY,
        folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
        artist_id INTEGER REFERENCES artists(id),
        album_id INTEGER REFERENCES albums(id),
        path TEXT NOT NULL UNIQUE,
        size INTEGER NOT NULL,
        mtime INTEGER NOT NULL,
        title TEXT NOT NULL,
        artist TEXT,
        album_artist TEXT,
        album TEXT,
        track_no INTEGER,
        disc_no INTEGER,
        year INTEGER,
        genre TEXT,
        duration_ms INTEGER NOT NULL,
        has_art INTEGER NOT NULL DEFAULT 0,
        added_at INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE INDEX tracks_folder_id_idx ON tracks(folder_id);
    CREATE INDEX tracks_artist_id_idx ON tracks(artist_id);
    CREATE INDEX tracks_album_id_idx ON tracks(album_id);
    CREATE INDEX tracks_title_idx ON tracks(title);
    CREATE TABLE plays (
        id INTEGER PRIMARY KEY,
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        played_at INTEGER NOT NULL
    );
    CREATE INDEX plays_track_id_idx ON plays(track_id);
    PRAGMA user_version = 1;
";

pub struct Db {
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub last_scan_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInput {
    pub folder_id: i64,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub duration_ms: i64,
    pub has_art: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Track {
    pub id: i64,
    pub folder_id: i64,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub duration_ms: i64,
    pub has_art: bool,
    pub added_at: i64,
    pub album_id: Option<i64>,
    pub artist_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Album {
    pub id: i64,
    pub name: String,
    pub album_artist: Option<String>,
    pub year: Option<i64>,
    pub track_count: i64,
    pub has_art: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub album_count: i64,
    pub track_count: i64,
    pub has_art: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryStats {
    pub folders: i64,
    pub tracks: i64,
    pub albums: i64,
    pub artists: i64,
    pub plays: i64,
}

impl Db {
    /// Opens the database, migrating it if needed.
    ///
    /// Safe to call concurrently on a fresh database: the migration takes the
    /// write lock up front (`BEGIN IMMEDIATE`) and re-reads `user_version`
    /// inside the transaction, so losers of the race see the finished schema
    /// instead of failing with "table already exists".
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(10))?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        let version = Self::read_user_version(&connection)?;
        Self::check_supported(version)?;
        if version == 0 {
            // WAL lets long scans write without blocking UI reads. It is
            // persistent in the file, so this is a no-op after the first open.
            // Not fatal: some filesystems cannot do WAL.
            if let Err(error) = connection.query_row("PRAGMA journal_mode = WAL", [], |row| {
                row.get::<_, String>(0)
            }) {
                tracing::warn!(%error, "could not enable WAL journal mode");
            }
            let migration = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let version = Self::read_user_version(&migration)?;
            Self::check_supported(version)?;
            if version == 0 {
                migration.execute_batch(VERSION_ONE_SCHEMA)?;
            }
            migration.commit()?;
        }
        Ok(Self { connection })
    }

    fn read_user_version(connection: &Connection) -> Result<i64> {
        connection.query_row("PRAGMA user_version", [], |row| row.get(0))
    }

    fn check_supported(version: i64) -> Result<()> {
        if version > SUPPORTED_SCHEMA_VERSION {
            return Err(rusqlite::Error::SqliteFailure(
                ffi::Error::new(ffi::SQLITE_ERROR),
                Some(format!(
                    "database schema version {version} is newer than supported version {SUPPORTED_SCHEMA_VERSION}"
                )),
            ));
        }
        Ok(())
    }

    pub fn user_version(&self) -> Result<i64> {
        self.connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
    }

    pub fn upsert_folder(&self, path: &str, last_scan_at: Option<i64>) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO folders(path, last_scan_at) VALUES (?1, ?2)
             ON CONFLICT(path) DO UPDATE SET last_scan_at = excluded.last_scan_at",
            params![path, last_scan_at],
        )?;
        self.connection
            .query_row("SELECT id FROM folders WHERE path = ?1", [path], |row| {
                row.get(0)
            })
    }

    /// Inserts the folder if it is not indexed yet and returns its id,
    /// leaving an existing row (and its `last_scan_at`) untouched.
    pub fn ensure_folder(&self, path: &str) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO folders(path, last_scan_at) VALUES (?1, NULL)
             ON CONFLICT(path) DO NOTHING",
            [path],
        )?;
        self.connection
            .query_row("SELECT id FROM folders WHERE path = ?1", [path], |row| {
                row.get(0)
            })
    }

    /// Looks a folder up without creating it.
    pub fn folder_id(&self, path: &str) -> Result<Option<i64>> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM folders WHERE path = ?1")?;
        let mut rows = statement.query([path])?;
        rows.next()?.map(|row| row.get(0)).transpose()
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, path, last_scan_at FROM folders ORDER BY path")?;
        let folders = statement
            .query_map([], |row| {
                Ok(Folder {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    last_scan_at: row.get(2)?,
                })
            })?
            .collect();
        folders
    }

    pub fn remove_folder(&self, path: &str) -> Result<bool> {
        let transaction = self.connection.unchecked_transaction()?;
        let removed = transaction.execute("DELETE FROM folders WHERE path = ?1", [path])? != 0;
        Self::cleanup_orphans(&transaction)?;
        transaction.commit()?;
        Ok(removed)
    }

    pub fn track_stamp(&self, path: &str) -> Result<Option<(i64, i64)>> {
        let mut statement = self
            .connection
            .prepare("SELECT size, mtime FROM tracks WHERE path = ?1")?;
        let mut rows = statement.query([path])?;
        rows.next()?
            .map(|row| Ok((row.get(0)?, row.get(1)?)))
            .transpose()
    }

    pub fn mark_folder_scanned(&self, folder_id: i64, scanned_at: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE folders SET last_scan_at = ?1 WHERE id = ?2",
            params![scanned_at, folder_id],
        )?;
        Ok(())
    }

    pub fn upsert_track(&self, track: &TrackInput) -> Result<i64> {
        let transaction = self.connection.unchecked_transaction()?;
        let artist_id = track
            .artist
            .as_deref()
            .filter(|name| !name.is_empty())
            .map(|name| Self::upsert_artist(&transaction, name))
            .transpose()?;
        let album_id = track
            .album
            .as_deref()
            .filter(|name| !name.is_empty())
            .map(|name| {
                Self::upsert_album(
                    &transaction,
                    name,
                    track.album_artist.as_deref(),
                    track.year,
                )
            })
            .transpose()?;

        transaction.execute(
            "INSERT INTO tracks (
                folder_id, artist_id, album_id, path, size, mtime, title, artist,
                album_artist, album, track_no, disc_no, year, genre, duration_ms, has_art
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
             )
             ON CONFLICT(path) DO UPDATE SET
                folder_id = excluded.folder_id,
                artist_id = excluded.artist_id,
                album_id = excluded.album_id,
                size = excluded.size,
                mtime = excluded.mtime,
                title = excluded.title,
                artist = excluded.artist,
                album_artist = excluded.album_artist,
                album = excluded.album,
                track_no = excluded.track_no,
                disc_no = excluded.disc_no,
                year = excluded.year,
                genre = excluded.genre,
                duration_ms = excluded.duration_ms,
                has_art = excluded.has_art",
            params![
                track.folder_id,
                artist_id,
                album_id,
                track.path,
                track.size,
                track.mtime,
                track.title,
                track.artist,
                track.album_artist,
                track.album,
                track.track_no,
                track.disc_no,
                track.year,
                track.genre,
                track.duration_ms,
                track.has_art,
            ],
        )?;
        let id = transaction.query_row(
            "SELECT id FROM tracks WHERE path = ?1",
            [&track.path],
            |row| row.get(0),
        )?;
        Self::cleanup_orphans(&transaction)?;
        transaction.commit()?;
        Ok(id)
    }

    /// Rewrites `Artist/Guest` rows using `credit`, then drops artist names
    /// that no longer have tracks. `credit` receives the current title and
    /// artist and returns the replacements. Rows it leaves unchanged are skipped.
    pub fn rewrite_collaborations<F>(&self, mut credit: F) -> Result<usize>
    where
        F: FnMut(&str, &str) -> (String, String),
    {
        let rows: Vec<(i64, String, String)> = {
            let mut statement = self
                .connection
                .prepare("SELECT id, title, artist FROM tracks WHERE artist LIKE '%/%'")?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<_>>()?;
            rows
        };
        if rows.is_empty() {
            return Ok(0);
        }

        let transaction = self.connection.unchecked_transaction()?;
        let mut changed = 0;
        for (id, title, artist) in rows {
            let (new_title, new_artist) = credit(&title, &artist);
            if new_artist.is_empty() || (new_title == title && new_artist == artist) {
                continue;
            }
            let artist_id = Self::upsert_artist(&transaction, &new_artist)?;
            changed += transaction.execute(
                "UPDATE tracks SET title = ?1, artist = ?2, artist_id = ?3 WHERE id = ?4",
                params![new_title, new_artist, artist_id, id],
            )?;
        }
        Self::cleanup_orphans(&transaction)?;
        transaction.commit()?;
        Ok(changed)
    }

    pub fn remove_missing_under(&self, folder_id: i64, present_paths: &[String]) -> Result<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let present: HashSet<&str> = present_paths.iter().map(String::as_str).collect();
        let candidates: Vec<(i64, String)> = {
            let mut statement =
                transaction.prepare("SELECT id, path FROM tracks WHERE folder_id = ?1")?;
            let candidates = statement
                .query_map([folder_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_>>()?;
            candidates
        };
        let mut removed = 0;
        for (id, path) in candidates {
            if !present.contains(path.as_str()) {
                removed += transaction.execute("DELETE FROM tracks WHERE id = ?1", [id])?;
            }
        }
        Self::cleanup_orphans(&transaction)?;
        transaction.commit()?;
        Ok(removed)
    }

    pub fn search(&self, query: &str, limit: u32, offset: u32) -> Result<Vec<Track>> {
        // Commas split the query. Every term must match somewhere on the track,
        // so "eminem, the slim shady lp" is that artist and that album.
        let terms: Vec<&str> = query
            .split(',')
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .take(8)
            .collect();
        let patterns: Vec<String> = terms.iter().map(|term| format!("%{term}%")).collect();
        let mut sql = String::from(
            "SELECT id, folder_id, path, size, mtime, title, artist, album_artist,
                    album, track_no, disc_no, year, genre, duration_ms, has_art, added_at,
                    album_id, artist_id
             FROM tracks",
        );
        for index in 0..patterns.len() {
            let clause = format!(
                "(title LIKE ?{0} OR artist LIKE ?{0} OR album_artist LIKE ?{0} OR album LIKE ?{0})",
                index + 1
            );
            if index == 0 {
                sql.push_str(" WHERE ");
            } else {
                sql.push_str(" AND ");
            }
            sql.push_str(&clause);
        }
        let limit_at = patterns.len() + 1;
        let offset_at = patterns.len() + 2;
        sql.push_str(&format!(
            " ORDER BY title COLLATE NOCASE, id LIMIT ?{limit_at} OFFSET ?{offset_at}"
        ));
        let mut statement = self.connection.prepare(&sql)?;
        let mut binds: Vec<&dyn rusqlite::types::ToSql> = patterns.iter().map(|p| p as _).collect();
        binds.push(&limit);
        binds.push(&offset);
        let tracks = statement
            .query_map(params_from_iter(binds), map_track)?
            .collect();
        tracks
    }

    pub fn list_tracks(&self, limit: u32, offset: u32) -> Result<Vec<Track>> {
        let mut statement = self.connection.prepare(
            "SELECT id, folder_id, path, size, mtime, title, artist, album_artist,
                    album, track_no, disc_no, year, genre, duration_ms, has_art, added_at,
                    album_id, artist_id
             FROM tracks
             ORDER BY title COLLATE NOCASE, id
             LIMIT ?1 OFFSET ?2",
        )?;
        let tracks = statement
            .query_map(params![limit, offset], map_track)?
            .collect();
        tracks
    }

    pub fn list_albums(&self, limit: u32, offset: u32) -> Result<Vec<Album>> {
        let mut statement = self.connection.prepare(
            "SELECT a.id, a.name, NULLIF(a.album_artist, ''), a.year,
                    COUNT(t.id), COALESCE(MAX(t.has_art), 0)
             FROM albums a
             JOIN tracks t ON t.album_id = a.id
             GROUP BY a.id
             ORDER BY a.name COLLATE NOCASE, a.album_artist COLLATE NOCASE
             LIMIT ?1 OFFSET ?2",
        )?;
        let albums = statement
            .query_map(params![limit, offset], |row| {
                Ok(Album {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    album_artist: row.get(2)?,
                    year: row.get(3)?,
                    track_count: row.get(4)?,
                    has_art: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect();
        albums
    }

    pub fn list_artists(&self, limit: u32, offset: u32) -> Result<Vec<Artist>> {
        let mut statement = self.connection.prepare(
            "SELECT a.id, a.name, COUNT(DISTINCT t.album_id), COUNT(t.id),
                    COALESCE(MAX(t.has_art), 0)
             FROM artists a
             JOIN tracks t ON t.artist_id = a.id
             GROUP BY a.id
             ORDER BY a.name COLLATE NOCASE
             LIMIT ?1 OFFSET ?2",
        )?;
        let artists = statement
            .query_map(params![limit, offset], |row| {
                Ok(Artist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    album_count: row.get(2)?,
                    track_count: row.get(3)?,
                    has_art: row.get::<_, i64>(4)? != 0,
                })
            })?
            .collect();
        artists
    }

    pub fn album_tracks(&self, album_id: i64) -> Result<Vec<Track>> {
        let mut statement = self.connection.prepare(
            "SELECT id, folder_id, path, size, mtime, title, artist, album_artist,
                    album, track_no, disc_no, year, genre, duration_ms, has_art, added_at,
                    album_id, artist_id
             FROM tracks
             WHERE album_id = ?1
             ORDER BY COALESCE(disc_no, 1), COALESCE(track_no, 2147483647),
                      title COLLATE NOCASE, id",
        )?;
        let tracks = statement.query_map([album_id], map_track)?.collect();
        tracks
    }

    pub fn artist_tracks(&self, artist_id: i64, limit: u32, offset: u32) -> Result<Vec<Track>> {
        let mut statement = self.connection.prepare(
            "SELECT id, folder_id, path, size, mtime, title, artist, album_artist,
                    album, track_no, disc_no, year, genre, duration_ms, has_art, added_at,
                    album_id, artist_id
             FROM tracks
             WHERE artist_id = ?1
             ORDER BY album COLLATE NOCASE, COALESCE(disc_no, 1),
                      COALESCE(track_no, 2147483647), title COLLATE NOCASE, id
             LIMIT ?2 OFFSET ?3",
        )?;
        let tracks = statement
            .query_map(params![artist_id, limit, offset], map_track)?
            .collect();
        tracks
    }

    /// One embedded picture for this artist: the first of their tracks that
    /// has artwork, in the same order as the artist page.
    pub fn artist_art_source(&self, artist_id: i64) -> Result<Option<String>> {
        match self.connection.query_row(
            "SELECT path FROM tracks
             WHERE artist_id = ?1 AND has_art = 1
             ORDER BY album COLLATE NOCASE, COALESCE(disc_no, 1),
                      COALESCE(track_no, 2147483647), id
             LIMIT 1",
            [artist_id],
            |row| row.get(0),
        ) {
            Ok(path) => Ok(Some(path)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn album_art_source(&self, album_id: i64) -> Result<Option<String>> {
        match self.connection.query_row(
            "SELECT path FROM tracks
             WHERE album_id = ?1 AND has_art = 1
             ORDER BY COALESCE(disc_no, 1), COALESCE(track_no, 2147483647), id
             LIMIT 1",
            [album_id],
            |row| row.get(0),
        ) {
            Ok(path) => Ok(Some(path)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Play queue for `scope`.
    ///
    /// `"library"` is every track in library order. `"artist"` is every track
    /// by the same artist. `"album"` (and any other value) is the track's
    /// album, or just the track when it has no album.
    pub fn queue_for_scope(
        &self,
        track_id: i64,
        scope: &str,
    ) -> Result<Option<(Vec<Track>, usize)>> {
        match scope {
            "library" => {
                let tracks = self.list_tracks(u32::MAX, 0)?;
                Ok(queue_index(tracks, track_id))
            }
            "artist" => self.queue_for_artist(track_id),
            _ => self.queue_for_track(track_id),
        }
    }

    /// Play queue for a single track: its album in album order and the
    /// track's index within it, or just the track when it has no album.
    pub fn queue_for_track(&self, track_id: i64) -> Result<Option<(Vec<Track>, usize)>> {
        let album_id = match self.connection.query_row(
            "SELECT album_id FROM tracks WHERE id = ?1",
            [track_id],
            |row| row.get::<_, Option<i64>>(0),
        ) {
            Ok(album_id) => album_id,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(error) => return Err(error),
        };
        if let Some(album_id) = album_id {
            let tracks = self.album_tracks(album_id)?;
            if let Some(index) = tracks.iter().position(|track| track.id == track_id) {
                return Ok(Some((tracks, index)));
            }
        }
        let mut statement = self.connection.prepare(
            "SELECT id, folder_id, path, size, mtime, title, artist, album_artist,
                    album, track_no, disc_no, year, genre, duration_ms, has_art, added_at,
                    album_id, artist_id
             FROM tracks WHERE id = ?1",
        )?;
        let tracks: Vec<Track> = statement
            .query_map([track_id], map_track)?
            .collect::<Result<_>>()?;
        Ok(Some((tracks, 0)))
    }

    /// Every track by this track's artist, in artist-page order.
    /// A track with no artist falls back to its album queue.
    fn queue_for_artist(&self, track_id: i64) -> Result<Option<(Vec<Track>, usize)>> {
        let artist_id = match self.connection.query_row(
            "SELECT artist_id FROM tracks WHERE id = ?1",
            [track_id],
            |row| row.get::<_, Option<i64>>(0),
        ) {
            Ok(artist_id) => artist_id,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(error) => return Err(error),
        };
        if let Some(artist_id) = artist_id {
            let tracks = self.artist_tracks(artist_id, u32::MAX, 0)?;
            if let Some(queue) = queue_index(tracks, track_id) {
                return Ok(Some(queue));
            }
        }
        self.queue_for_track(track_id)
    }

    pub fn stats(&self) -> Result<LibraryStats> {
        self.connection.query_row(
            "SELECT
                (SELECT COUNT(*) FROM folders),
                (SELECT COUNT(*) FROM tracks),
                (SELECT COUNT(*) FROM albums),
                (SELECT COUNT(*) FROM artists),
                (SELECT COUNT(*) FROM plays)",
            [],
            |row| {
                Ok(LibraryStats {
                    folders: row.get(0)?,
                    tracks: row.get(1)?,
                    albums: row.get(2)?,
                    artists: row.get(3)?,
                    plays: row.get(4)?,
                })
            },
        )
    }

    pub fn record_play(&self, track_id: i64, played_at: i64) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO plays(track_id, played_at) VALUES (?1, ?2)",
            params![track_id, played_at],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    fn upsert_artist(connection: &Connection, name: &str) -> Result<i64> {
        connection.execute(
            "INSERT INTO artists(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            [name],
        )?;
        connection.query_row("SELECT id FROM artists WHERE name = ?1", [name], |row| {
            row.get(0)
        })
    }

    fn upsert_album(
        connection: &Connection,
        name: &str,
        album_artist: Option<&str>,
        year: Option<i64>,
    ) -> Result<i64> {
        let album_artist = album_artist.unwrap_or("");
        connection.execute(
            "INSERT INTO albums(name, album_artist, year) VALUES (?1, ?2, ?3)
             ON CONFLICT(name, album_artist) DO UPDATE SET year = excluded.year",
            params![name, album_artist, year],
        )?;
        connection.query_row(
            "SELECT id FROM albums WHERE name = ?1 AND album_artist = ?2",
            params![name, album_artist],
            |row| row.get(0),
        )
    }

    fn cleanup_orphans(connection: &Connection) -> Result<()> {
        connection.execute(
            "DELETE FROM albums WHERE NOT EXISTS (
                SELECT 1 FROM tracks WHERE tracks.album_id = albums.id
            )",
            [],
        )?;
        connection.execute(
            "DELETE FROM artists WHERE NOT EXISTS (
                SELECT 1 FROM tracks WHERE tracks.artist_id = artists.id
            )",
            [],
        )?;
        Ok(())
    }
}

fn queue_index(tracks: Vec<Track>, track_id: i64) -> Option<(Vec<Track>, usize)> {
    let index = tracks.iter().position(|track| track.id == track_id)?;
    Some((tracks, index))
}

fn map_track(row: &rusqlite::Row<'_>) -> Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        folder_id: row.get(1)?,
        path: row.get(2)?,
        size: row.get(3)?,
        mtime: row.get(4)?,
        title: row.get(5)?,
        artist: row.get(6)?,
        album_artist: row.get(7)?,
        album: row.get(8)?,
        track_no: row.get(9)?,
        disc_no: row.get(10)?,
        year: row.get(11)?,
        genre: row.get(12)?,
        duration_ms: row.get(13)?,
        has_art: row.get::<_, i64>(14)? != 0,
        added_at: row.get(15)?,
        album_id: row.get(16)?,
        artist_id: row.get(17)?,
    })
}
