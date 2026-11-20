//! Folder scanning.
//!
//! Every filesystem call that can touch a (possibly stalled) network mount —
//! directory listing, `stat`, and tag reads — runs through [`TimedIo`], a
//! fixed-size shared worker pool. A call that does not finish within the
//! timeout is abandoned by the caller, but its worker (and its pool permit)
//! stays occupied until the syscall really returns. Once every worker is
//! occupied, further timed calls fail immediately instead of spawning more
//! threads, so a dead mount can strand at most [`IO_WORKERS`] OS threads in
//! total rather than one per timeout.
//!
//! Directory traversal is an explicit, per-directory timed walk. If any
//! directory cannot be listed, discovery is *incomplete*; an incomplete scan
//! never prunes indexed tracks (`remove_missing_under`) because a missing
//! path may only mean the subtree was unreachable.

use std::fs::Metadata;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::db::{Db, TrackInput};
use crate::meta;

const AUDIO_EXTENSIONS: &[&str] = &["flac", "mp3", "m4a", "aac", "wav", "ogg", "opus"];
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on OS threads that may ever be blocked inside timed I/O.
const IO_WORKERS: usize = 4;

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
    /// `false` when any directory could not be listed in time; pruning is skipped then.
    pub discovery_complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntryInfo {
    pub path: PathBuf,
    pub kind: EntryKind,
}

/// Lists one directory. Injectable so traversal failures can be tested.
pub type ListDir = dyn Fn(&Path) -> io::Result<Vec<DirEntryInfo>> + Send + Sync;

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Permit(Arc<AtomicUsize>);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Fixed-size worker pool with a per-call timeout.
///
/// Permits are counted per accepted job and released only when the job body
/// actually finishes, so `in_flight() <= workers` always holds and every
/// accepted job has a worker ready to run it.
pub struct TimedIo {
    sender: mpsc::Sender<Job>,
    in_flight: Arc<AtomicUsize>,
    capacity: usize,
    timeout: Duration,
}

impl TimedIo {
    pub fn new(workers: usize, timeout: Duration) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut capacity = 0;
        for index in 0..workers.max(1) {
            let receiver = receiver.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("finalboss-io-{index}"))
                .spawn(move || loop {
                    let job = {
                        let guard = receiver.lock().unwrap_or_else(|e| e.into_inner());
                        guard.recv()
                    };
                    match job {
                        Ok(job) => {
                            let _ = catch_unwind(AssertUnwindSafe(job));
                        }
                        Err(_) => return,
                    }
                });
            match spawned {
                Ok(_) => capacity += 1,
                Err(error) => tracing::warn!(%error, "could not start I/O worker thread"),
            }
        }
        Self {
            sender,
            in_flight: Arc::new(AtomicUsize::new(0)),
            capacity,
            timeout,
        }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    fn try_acquire(&self) -> Option<Permit> {
        let mut current = self.in_flight.load(Ordering::SeqCst);
        loop {
            if current >= self.capacity {
                return None;
            }
            match self.in_flight.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Some(Permit(self.in_flight.clone())),
                Err(actual) => current = actual,
            }
        }
    }

    pub fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = self.try_acquire().ok_or_else(|| {
            anyhow!("filesystem I/O unavailable: all workers are blocked on stalled operations")
        })?;
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let job: Job = Box::new(move || {
            let _permit = permit;
            let _ = result_sender.send(operation());
        });
        self.sender
            .send(job)
            .map_err(|_| anyhow!("filesystem I/O workers are not running"))?;
        match result_receiver.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(anyhow!(
                "file operation timed out after {} seconds",
                self.timeout.as_secs_f32()
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(anyhow!("file operation worker failed"))
            }
        }
    }
}

fn default_io() -> &'static TimedIo {
    static IO: OnceLock<TimedIo> = OnceLock::new();
    IO.get_or_init(|| TimedIo::new(IO_WORKERS, IO_TIMEOUT))
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

pub fn read_dir_entries(dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let kind = if file_type.is_dir() {
            EntryKind::Dir
        } else if file_type.is_file() {
            EntryKind::File
        } else {
            // Symlinks and special files are not followed or indexed.
            EntryKind::Other
        };
        entries.push(DirEntryInfo {
            path: entry.path(),
            kind,
        });
    }
    Ok(entries)
}

pub fn run(
    db: Db,
    folder: &Path,
    full: bool,
    cancel: Arc<AtomicBool>,
    on_progress: impl FnMut(ScanProgress),
) -> Result<ScanResult> {
    run_with(
        db,
        folder,
        full,
        cancel,
        default_io(),
        Arc::new(read_dir_entries),
        on_progress,
    )
}

fn cancelled_result(scanned: usize, indexed: usize, unavailable: usize) -> ScanResult {
    ScanResult {
        scanned,
        indexed,
        unavailable,
        removed: 0,
        cancelled: true,
        discovery_complete: false,
    }
}

pub fn run_with(
    db: Db,
    folder: &Path,
    full: bool,
    cancel: Arc<AtomicBool>,
    io: &TimedIo,
    list_dir: Arc<ListDir>,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanResult> {
    // A stale scan (e.g. for a folder that was just removed) must not touch the DB.
    if cancel.load(Ordering::Relaxed) {
        return Ok(cancelled_result(0, 0, 0));
    }
    let folder_path = folder.to_string_lossy().into_owned();
    // Scans only index folders that are already registered. Looking the folder
    // up (never inserting) keeps a stale scan from resurrecting a folder that
    // was removed in the meantime.
    let Some(folder_id) = db.folder_id(&folder_path)? else {
        tracing::info!(folder = %folder.display(), "folder is no longer registered; skipping scan");
        return Ok(ScanResult {
            scanned: 0,
            indexed: 0,
            unavailable: 0,
            removed: 0,
            cancelled: false,
            discovery_complete: false,
        });
    };

    on_progress(ScanProgress {
        scanning: true,
        scanned: 0,
        total: 0,
    });
    let discovery = discover_audio_files(folder, io, &list_dir, &cancel);
    if discovery.cancelled || cancel.load(Ordering::Relaxed) {
        return Ok(cancelled_result(0, 0, 0));
    }
    let paths = discovery.paths;
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
            return Ok(cancelled_result(index, indexed, unavailable));
        }

        let path_string = path.to_string_lossy().into_owned();
        present_paths.push(path_string.clone());
        match timed_metadata(io, path.clone()) {
            Ok(metadata) => {
                let size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
                let mtime = modified_seconds(&metadata)?;
                let existing = db.track_stamp(&path_string)?;
                if !should_skip_unchanged(existing, size, mtime, full) {
                    match timed_meta_read(io, path.clone()) {
                        Ok(track_meta) => {
                            if cancel.load(Ordering::Relaxed) {
                                return Ok(cancelled_result(index, indexed, unavailable));
                            }
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

    if cancel.load(Ordering::Relaxed) {
        return Ok(cancelled_result(total, indexed, unavailable));
    }

    // Only prune after a complete traversal: an unreachable subtree is not a deletion.
    let removed = if discovery.complete {
        let removed = db.remove_missing_under(folder_id, &present_paths)?;
        db.mark_folder_scanned(folder_id, unix_seconds(SystemTime::now())?)?;
        removed
    } else {
        tracing::warn!(
            folder = %folder.display(),
            "folder traversal incomplete; keeping existing index entries"
        );
        0
    };
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
        discovery_complete: discovery.complete,
    })
}

struct Discovery {
    paths: Vec<PathBuf>,
    complete: bool,
    cancelled: bool,
}

fn discover_audio_files(
    folder: &Path,
    io: &TimedIo,
    list_dir: &Arc<ListDir>,
    cancel: &AtomicBool,
) -> Discovery {
    let mut discovery = Discovery {
        paths: Vec::new(),
        complete: true,
        cancelled: false,
    };
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            discovery.cancelled = true;
            break;
        }
        let lister = list_dir.clone();
        let listing_path = dir.clone();
        let listed = io.call(move || {
            lister(&listing_path)
                .with_context(|| format!("could not list {}", listing_path.display()))
        });
        match listed {
            Ok(entries) => {
                for entry in entries {
                    match entry.kind {
                        EntryKind::Dir => pending.push(entry.path),
                        EntryKind::File if is_supported_audio(&entry.path) => {
                            discovery.paths.push(entry.path);
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => {
                discovery.complete = false;
                tracing::warn!(dir = %dir.display(), %error, "could not inspect folder");
            }
        }
    }
    discovery.paths.sort();
    discovery
}

fn timed_metadata(io: &TimedIo, path: PathBuf) -> Result<Metadata> {
    io.call(move || {
        std::fs::metadata(&path).with_context(|| format!("could not stat {}", path.display()))
    })
}

fn timed_meta_read(io: &TimedIo, path: PathBuf) -> Result<meta::TrackMeta> {
    io.call(move || meta::read(&path))
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
