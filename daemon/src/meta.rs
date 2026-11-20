use std::path::Path;

use anyhow::{Context, Result};
use lofty::file::TaggedFileExt;
use lofty::prelude::{Accessor, AudioFile, ItemKey};
use lofty::probe::read_from_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackMeta {
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

pub fn read(path: &Path) -> Result<TrackMeta> {
    let tagged = read_from_path(path)
        .with_context(|| format!("could not read audio metadata from {}", path.display()))?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let fallback_title = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown")
        .to_owned();

    Ok(TrackMeta {
        title: tag
            .and_then(Accessor::title)
            .map(|value| value.into_owned())
            .unwrap_or(fallback_title),
        artist: tag
            .and_then(Accessor::artist)
            .map(|value| value.into_owned()),
        album_artist: tag
            .and_then(|tag| tag.get_string(ItemKey::AlbumArtist))
            .map(str::to_owned),
        album: tag
            .and_then(Accessor::album)
            .map(|value| value.into_owned()),
        track_no: tag.and_then(Accessor::track).map(i64::from),
        disc_no: tag.and_then(Accessor::disk).map(i64::from),
        year: tag
            .and_then(Accessor::date)
            .map(|timestamp| i64::from(timestamp.year)),
        genre: tag
            .and_then(Accessor::genre)
            .map(|value| value.into_owned()),
        duration_ms: i64::try_from(tagged.properties().duration().as_millis()).unwrap_or(i64::MAX),
        has_art: tag.is_some_and(|tag| !tag.pictures().is_empty()),
    })
}
