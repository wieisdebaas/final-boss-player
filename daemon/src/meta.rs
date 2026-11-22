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
    let title = tag
        .and_then(Accessor::title)
        .map(|value| value.into_owned())
        .unwrap_or(fallback_title);
    let artist = tag
        .and_then(Accessor::artist)
        .map(|value| value.into_owned());
    let (title, artist) = credit_featured_artists(title, artist);

    Ok(TrackMeta {
        title,
        artist,
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

/// Collapses `Artist/Guest` tags onto the first name.
///
/// `50 Cent/Nate Dogg` becomes artist `50 Cent` and the title gains
/// ` - featuring Nate Dogg`. Guests already named in the title are skipped
/// so an existing `(feat. …)` credit is not repeated.
pub fn credit_featured_artists(title: String, artist: Option<String>) -> (String, Option<String>) {
    let Some(raw) = artist else {
        return (title, None);
    };
    if !raw.contains('/') {
        return (title, Some(raw));
    }

    let parts: Vec<&str> = raw
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let Some((primary, guests)) = parts.split_first() else {
        return (title, Some(raw));
    };
    if guests.is_empty() {
        return (title, Some((*primary).to_owned()));
    }

    let title_lower = title.to_lowercase();
    let missing: Vec<&str> = guests
        .iter()
        .copied()
        .filter(|guest| !title_lower.contains(&guest.to_lowercase()))
        .collect();
    if missing.is_empty() {
        return (title, Some((*primary).to_owned()));
    }

    (
        format!("{title} - featuring {}", join_names(&missing)),
        Some((*primary).to_owned()),
    )
}

fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [first, second] => format!("{first} & {second}"),
        _ => {
            let last = names.len() - 1;
            let mut text = names[..last].join(", ");
            text.push_str(" & ");
            text.push_str(names[last]);
            text
        }
    }
}
