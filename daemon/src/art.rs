use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::ImageReader;
use lofty::file::TaggedFileExt;
use lofty::probe::read_from_path;

const MAX_DIMENSION: u32 = 300;
static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

/// Returns a deterministic cache location without incorporating source paths.
pub fn cache_path(art_dir: &Path, album_id: i64) -> PathBuf {
    art_dir.join(format!("album-{album_id}.jpg"))
}

/// Extracts and caches the first embedded picture as a bounded JPEG thumbnail.
pub fn get_or_create(art_dir: &Path, album_id: i64, source: &Path) -> Result<PathBuf> {
    let destination = cache_path(art_dir, album_id);
    if destination.is_file() {
        return Ok(destination);
    }

    let tagged = read_from_path(source)
        .with_context(|| format!("could not read artwork from {}", source.display()))?;
    let picture = tagged
        .primary_tag()
        .or_else(|| tagged.first_tag())
        .and_then(|tag| tag.pictures().first())
        .context("track has no embedded artwork")?;
    let image = ImageReader::new(std::io::Cursor::new(picture.data()))
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
            return Ok(destination);
        }
        bail!(
            "could not move artwork thumbnail to {}: {error}",
            destination.display()
        );
    }
    Ok(destination)
}
