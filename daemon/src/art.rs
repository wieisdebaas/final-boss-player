use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::ImageReader;
use lofty::file::TaggedFileExt;
use lofty::probe::read_from_path;

use crate::scan::TimedIo;

const MAX_DIMENSION: u32 = 300;
static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);
const TAG_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Threads that stalled tag reads may strand; further reads then fail fast.
const TAG_READ_WORKERS: usize = 2;

fn art_io() -> &'static TimedIo {
    static IO: OnceLock<TimedIo> = OnceLock::new();
    IO.get_or_init(|| TimedIo::new(TAG_READ_WORKERS, TAG_READ_TIMEOUT))
}

/// Reads the first embedded picture's bytes from `source` through `io`, so a
/// stalled network mount cannot block the caller (or leak a thread per call).
pub fn read_picture(io: &TimedIo, source: &Path) -> Result<Vec<u8>> {
    let path = source.to_path_buf();
    io.call(move || {
        let tagged = read_from_path(&path)
            .with_context(|| format!("could not read artwork from {}", path.display()))?;
        let picture = tagged
            .primary_tag()
            .or_else(|| tagged.first_tag())
            .and_then(|tag| tag.pictures().first())
            .context("track has no embedded artwork")?;
        Ok(picture.data().to_vec())
    })
}

/// Returns a deterministic cache location without incorporating source paths.
pub fn cache_path(art_dir: &Path, album_id: i64) -> PathBuf {
    art_dir.join(format!("album-{album_id}.jpg"))
}

/// Artist thumbnails are cached separately so they never collide with album ids.
pub fn artist_cache_path(art_dir: &Path, artist_id: i64) -> PathBuf {
    art_dir.join(format!("artist-{artist_id}.jpg"))
}

/// Extracts and caches the first embedded picture as a bounded JPEG thumbnail.
pub fn get_or_create(art_dir: &Path, album_id: i64, source: &Path) -> Result<PathBuf> {
    write_thumbnail(&cache_path(art_dir, album_id), art_dir, source)
}

/// Same thumbnail cache as [`get_or_create`], keyed by artist id.
pub fn get_or_create_artist(art_dir: &Path, artist_id: i64, source: &Path) -> Result<PathBuf> {
    write_thumbnail(&artist_cache_path(art_dir, artist_id), art_dir, source)
}

fn write_thumbnail(destination: &Path, art_dir: &Path, source: &Path) -> Result<PathBuf> {
    if destination.is_file() {
        return Ok(destination.to_path_buf());
    }

    let picture = read_picture(art_io(), source)?;
    let image = ImageReader::new(std::io::Cursor::new(picture))
        .with_guessed_format()
        .context("could not determine embedded artwork format")?
        .decode()
        .context("could not decode embedded artwork")?
        .thumbnail(MAX_DIMENSION, MAX_DIMENSION);

    fs::create_dir_all(art_dir)
        .with_context(|| format!("could not create {}", art_dir.display()))?;
    let temporary = destination.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let file = fs::File::create(&temporary)
        .with_context(|| format!("could not create {}", temporary.display()))?;
    JpegEncoder::new_with_quality(file, 85)
        .encode_image(&image)
        .context("could not encode artwork thumbnail")?;
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        if destination.is_file() {
            return Ok(destination.to_path_buf());
        }
        bail!(
            "could not move artwork thumbnail to {}: {error}",
            destination.display()
        );
    }
    Ok(destination.to_path_buf())
}
