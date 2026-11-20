use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use walkdir::WalkDir;

use crate::db::{Db, TrackInput};
use crate::meta;

const AUDIO_EXTENSIONS: &[&str] = &["flac", "mp3", "m4a", "aac", "wav", "ogg", "opus"];
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScanProgress {
    pub scanning: bool,
    pub scanned: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanResult {
    pub scanned: usize,
    pub indexed: usize,
    pub unavailable: usize,
    pub removed: usize,
    pub cancelled: bool,
}

pub fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            AUDIO_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

pub fn should_skip_unchanged(
    existing: Option<(i64, i64)>,
    size: i64,
    mtime: i64,
    full: bool,
) -> bool {
    !full && existing == Some((size, mtime))
}

pub fn run(
    db: Db,
    folder: &Path,
    full: bool,
    cancel: Arc<AtomicBool>,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanResult> {
    let folder_path = folder.to_string_lossy().into_owned();
    let folder_id = db.upsert_folder(&folder_path, None)?;
    let paths = discover_audio_files(folder);
    let total = paths.len();
    let mut present_paths = Vec::with_capacity(total);
    let mut indexed = 0;
    let mut unavailable = 0;

    on_progress(ScanProgress {
        scanning: true,
        scanned: 0,
        total,
    });

    for (index, path) in paths.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(ScanResult {
                scanned: index,
                indexed,
                unavailable,
                removed: 0,
                cancelled: true,
            });
        }

        let path_string = path.to_string_lossy().into_owned();
        present_paths.push(path_string.clone());
        match timed_metadata(path.clone()) {
            Ok(metadata) => {
                let size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
                let mtime = modified_seconds(&metadata)?;
                let existing = db.track_stamp(&path_string)?;
                if !should_skip_unchanged(existing, size, mtime, full) {
                    match timed_meta_read(path.clone()) {
                        Ok(track_meta) => {
                            db.upsert_track(&TrackInput {
                                folder_id,
                                path: path_string,
                                size,
                                mtime,
                                title: track_meta.title,
                                artist: track_meta.artist,
                                album_artist: track_meta.album_artist,
                                album: track_meta.album,
                                track_no: track_meta.track_no,
                                disc_no: track_meta.disc_no,
                                year: track_meta.year,
                                genre: track_meta.genre,
                                duration_ms: track_meta.duration_ms,
                                has_art: track_meta.has_art,
                            })?;
                            indexed += 1;
                        }
                        Err(error) => {
                            unavailable += 1;
                            tracing::warn!(path = %path.display(), %error, "skipping unreadable audio file");
                        }
                    }
                }
            }
            Err(error) => {
                unavailable += 1;
                tracing::warn!(path = %path.display(), %error, "skipping unavailable audio file");
            }
        }

        on_progress(ScanProgress {
            scanning: true,
            scanned: index + 1,
            total,
        });
    }

    let removed = db.remove_missing_under(folder_id, &present_paths)?;
    db.mark_folder_scanned(folder_id, unix_seconds(SystemTime::now())?)?;
    on_progress(ScanProgress {
        scanning: false,
        scanned: total,
        total,
    });

    Ok(ScanResult {
        scanned: total,
        indexed,
        unavailable,
        removed,
        cancelled: false,
    })
}

fn discover_audio_files(folder: &Path) -> Vec<PathBuf> {
    WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_type().is_file() && is_supported_audio(entry.path()) => {
                Some(entry.into_path())
            }
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(%error, "could not inspect folder entry");
                None
            }
        })
        .collect()
}

fn timed_metadata(path: PathBuf) -> Result<Metadata> {
    timed_io(move || {
        std::fs::metadata(&path).with_context(|| format!("could not stat {}", path.display()))
    })
}

fn timed_meta_read(path: PathBuf) -> Result<meta::TrackMeta> {
    timed_io(move || meta::read(&path))
}

fn timed_io<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(operation());
    });
    receiver
        .recv_timeout(IO_TIMEOUT)
        .map_err(|_| anyhow!("file operation timed out after 5 seconds"))?
}

fn modified_seconds(metadata: &Metadata) -> Result<i64> {
    unix_seconds(
        metadata
            .modified()
            .context("file has no modification time")?,
    )
}

fn unix_seconds(time: SystemTime) -> Result<i64> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .context("time is before the Unix epoch")?
            .as_secs(),
    )
    .context("timestamp exceeds SQLite integer range")
}
