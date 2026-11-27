//! Fill missing genre and cover art from MusicBrainz, Cover Art Archive, and
//! AcoustID. Audio files are never written. A lookup is remembered so the next
//! scan does not repeat it.
//!
//! MusicBrainz allows one request per second. The real client waits between
//! calls. Files that already have an artist and an album are searched by those
//! tags. AcoustID runs only when those tags are too thin to search on.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::art;
use crate::db::{self, album_key, Db};
use crate::fingerprint;
use crate::state::AppState;

/// AcoustID application key for Final Boss Player.
///
/// This identifies the app's quota and is meant to ship with the program so
/// every install can fingerprint. Other applications need their own key.
const ACOUSTID_API_KEY: &str = "NjAjoPhUOg";
const USER_AGENT: &str =
    "FinalBossPlayer/0.1.0 ( https://github.com/wieisdebaas/final-boss-player )";
const REQUEST_GAP: Duration = Duration::from_millis(1100);
const MIN_RELEASE_SCORE: u64 = 80;
const MIN_ACOUSTID_SCORE: f64 = 0.5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataProgress {
    pub looking: bool,
    pub looked: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub artist: String,
    pub album: String,
    pub duration_secs: u32,
    pub fingerprint: Option<String>,
    pub fetch_art: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Found {
        release_mbid: String,
        genre: Option<String>,
        art: Option<Vec<u8>>,
    },
    NotFound,
    Retry,
}

pub struct Fetched {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait Http {
    fn get(&self, url: &str) -> Result<Fetched>;
    fn post(&self, url: &str, content_type: &str, body: &str) -> Result<Fetched>;
}

struct ReleaseHit {
    id: String,
    release_group: Option<String>,
}

enum Parsed<T> {
    Value(T),
    Empty,
    Retry,
}

pub fn tags_searchable(artist: &str, album: &str) -> bool {
    !artist.trim().is_empty() && !album.trim().is_empty()
}

pub fn release_search_url(artist: &str, album: &str) -> String {
    let query = format!(
        "release:{} AND artist:{}",
        quote_lucene(album),
        quote_lucene(artist)
    );
    format!(
        "https://musicbrainz.org/ws/2/release/?query={}&fmt=json&limit=5",
        percent_encode(&query)
    )
}

pub fn resolve(http: &dyn Http, query: &Query) -> Outcome {
    let hit = if tags_searchable(&query.artist, &query.album) {
        match get_ok(http, &release_search_url(&query.artist, &query.album)) {
            Parsed::Retry => return Outcome::Retry,
            Parsed::Empty => return Outcome::NotFound,
            Parsed::Value(body) => match best_release(&body) {
                Some(hit) => hit,
                None => return Outcome::NotFound,
            },
        }
    } else {
        let Some(fingerprint) = query.fingerprint.as_deref() else {
            return Outcome::Retry;
        };
        match acoustid_release(http, query.duration_secs, fingerprint) {
            Parsed::Retry => return Outcome::Retry,
            Parsed::Empty => return Outcome::NotFound,
            Parsed::Value(hit) => hit,
        }
    };

    let genre = match fetch_genre(http, &hit) {
        Parsed::Retry => return Outcome::Retry,
        Parsed::Empty => None,
        Parsed::Value(genre) => Some(genre),
    };
    let art = if query.fetch_art {
        match fetch_art(http, &hit) {
            Parsed::Retry => return Outcome::Retry,
            Parsed::Empty => None,
            Parsed::Value(bytes) => Some(bytes),
        }
    } else {
        None
    };
    Outcome::Found {
        release_mbid: hit.id,
        genre,
        art,
    }
}

/// Walks albums that are still missing a genre or a cover. `generation` is
/// stale when a newer lookup, or a cancel, has replaced this one.
pub fn enrich_library(
    db_path: &Path,
    art_dir: &Path,
    state: &AppState,
    generation: u64,
    mut report: impl FnMut(MetadataProgress),
) {
    if !state.metadata_is_current(generation) {
        return;
    }
    let db = match Db::open(db_path) {
        Ok(db) => db,
        Err(error) => {
            tracing::warn!(%error, "metadata lookup could not open the library");
            return;
        }
    };
    if let Err(error) = db.refill_cached_genres() {
        tracing::warn!(%error, "could not reapply cached genres");
    }
    let gaps = match db.albums_missing_metadata() {
        Ok(gaps) => gaps,
        Err(error) => {
            tracing::warn!(%error, "could not list albums that need metadata");
            return;
        }
    };
    let total = u32::try_from(gaps.len()).unwrap_or(u32::MAX);
    report(MetadataProgress {
        looking: true,
        looked: 0,
        total,
    });
    let http = match ReqwestHttp::new() {
        Ok(http) => http,
        Err(error) => {
            tracing::warn!(%error, "metadata lookup could not start http");
            report(MetadataProgress {
                looking: false,
                looked: 0,
                total,
            });
            return;
        }
    };

    for (index, gap) in gaps.iter().enumerate() {
        if !state.metadata_is_current(generation) {
            return;
        }
        let looked = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let fingerprint = if tags_searchable(&gap.artist, &gap.name) {
            None
        } else {
            match fingerprint::fingerprint_file(Path::new(&gap.sample_path)) {
                Ok(fingerprint) => Some(fingerprint),
                Err(error) => {
                    tracing::warn!(
                        album = %gap.name,
                        path = %gap.sample_path,
                        %error,
                        "could not fingerprint album"
                    );
                    report(MetadataProgress {
                        looking: true,
                        looked,
                        total,
                    });
                    continue;
                }
            }
        };
        let duration_secs = u32::try_from(gap.duration_ms / 1000).unwrap_or(1).max(1);
        let outcome = resolve(
            &http,
            &Query {
                artist: gap.artist.clone(),
                album: gap.name.clone(),
                duration_secs,
                fingerprint,
                fetch_art: gap.needs_art,
            },
        );
        if let Err(error) = apply_outcome(&db, art_dir, gap, outcome) {
            tracing::warn!(album = %gap.name, %error, "could not store metadata lookup");
        }
        if state.metadata_is_current(generation) {
            report(MetadataProgress {
                looking: true,
                looked,
                total,
            });
        }
    }
    if state.metadata_is_current(generation) {
        report(MetadataProgress {
            looking: false,
            looked: total,
            total,
        });
    }
}

fn apply_outcome(db: &Db, art_dir: &Path, gap: &db::AlbumGap, outcome: Outcome) -> Result<()> {
    let key = album_key(&gap.artist, &gap.name);
    match outcome {
        Outcome::Retry => Ok(()),
        Outcome::NotFound => {
            db.remember_lookup(&key, false, None, None)?;
            Ok(())
        }
        Outcome::Found {
            release_mbid,
            genre,
            art,
        } => {
            if gap.needs_genre {
                if let Some(genre) = genre.as_deref() {
                    db.fill_empty_genre(gap.id, genre)?;
                }
            }
            if let Some(bytes) = art.as_deref() {
                art::store_cover(art_dir, gap.id, bytes)
                    .with_context(|| format!("could not cache cover for {}", gap.name))?;
                db.mark_album_cover(gap.id)?;
            }
            db.remember_lookup(&key, true, Some(&release_mbid), genre.as_deref())?;
            Ok(())
        }
    }
}

fn acoustid_release(http: &dyn Http, duration_secs: u32, fingerprint: &str) -> Parsed<ReleaseHit> {
    let body = format!(
        "client={}&duration={}&fingerprint={}&meta=recordings+releases+releasegroups",
        percent_encode(ACOUSTID_API_KEY),
        duration_secs.max(1),
        percent_encode(fingerprint)
    );
    let fetched = match http.post(
        "https://api.acoustid.org/v2/lookup",
        "application/x-www-form-urlencoded",
        &body,
    ) {
        Ok(fetched) => fetched,
        Err(_) => return Parsed::Retry,
    };
    if fetched.status == 429 || fetched.status >= 500 {
        return Parsed::Retry;
    }
    if fetched.status != 200 {
        return Parsed::Retry;
    }
    match best_acoustid(&fetched.body) {
        Parsed::Retry => Parsed::Retry,
        Parsed::Empty => Parsed::Empty,
        Parsed::Value((release_id, release_group)) => {
            if let Some(id) = release_id {
                Parsed::Value(ReleaseHit { id, release_group })
            } else if let Some(group) = release_group {
                match browse_release(http, &group) {
                    Parsed::Value(id) => Parsed::Value(ReleaseHit {
                        id,
                        release_group: Some(group),
                    }),
                    Parsed::Empty => Parsed::Empty,
                    Parsed::Retry => Parsed::Retry,
                }
            } else {
                Parsed::Empty
            }
        }
    }
}

fn browse_release(http: &dyn Http, release_group: &str) -> Parsed<String> {
    let url = format!(
        "https://musicbrainz.org/ws/2/release?release-group={}&fmt=json&limit=1",
        percent_encode(release_group)
    );
    match get_ok(http, &url) {
        Parsed::Value(body) => first_release_id(&body)
            .map(Parsed::Value)
            .unwrap_or(Parsed::Empty),
        Parsed::Empty => Parsed::Empty,
        Parsed::Retry => Parsed::Retry,
    }
}

fn fetch_genre(http: &dyn Http, hit: &ReleaseHit) -> Parsed<String> {
    let url = if let Some(group) = &hit.release_group {
        format!(
            "https://musicbrainz.org/ws/2/release-group/{}?inc=genres&fmt=json",
            percent_encode(group)
        )
    } else {
        format!(
            "https://musicbrainz.org/ws/2/release/{}?inc=genres&fmt=json",
            percent_encode(&hit.id)
        )
    };
    match get_ok(http, &url) {
        Parsed::Value(body) => best_genre(&body)
            .map(Parsed::Value)
            .unwrap_or(Parsed::Empty),
        Parsed::Empty => Parsed::Empty,
        Parsed::Retry => Parsed::Retry,
    }
}

fn fetch_art(http: &dyn Http, hit: &ReleaseHit) -> Parsed<Vec<u8>> {
    let release_url = format!("https://coverartarchive.org/release/{}/front-500", hit.id);
    match get_bytes(http, &release_url) {
        Parsed::Value(bytes) => Parsed::Value(bytes),
        Parsed::Retry => Parsed::Retry,
        Parsed::Empty => {
            let Some(group) = &hit.release_group else {
                return Parsed::Empty;
            };
            let group_url = format!("https://coverartarchive.org/release-group/{group}/front-500");
            get_bytes(http, &group_url)
        }
    }
}

fn get_ok(http: &dyn Http, url: &str) -> Parsed<Vec<u8>> {
    match http.get(url) {
        Err(_) => Parsed::Retry,
        Ok(fetched) if fetched.status == 429 || fetched.status >= 500 => Parsed::Retry,
        Ok(fetched) if fetched.status != 200 => Parsed::Empty,
        Ok(fetched) => Parsed::Value(fetched.body),
    }
}

fn get_bytes(http: &dyn Http, url: &str) -> Parsed<Vec<u8>> {
    match get_ok(http, url) {
        Parsed::Value(body) if body.is_empty() => Parsed::Empty,
        other => other,
    }
}

fn best_release(body: &[u8]) -> Option<ReleaseHit> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let mut best: Option<(u64, ReleaseHit)> = None;
    for release in json.get("releases")?.as_array()? {
        let score = json_u64(release.get("score"));
        if score < MIN_RELEASE_SCORE {
            continue;
        }
        let id = release.get("id")?.as_str()?.to_owned();
        let release_group = release
            .get("release-group")
            .and_then(|group| group.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if best.as_ref().is_none_or(|(current, _)| score > *current) {
            best = Some((score, ReleaseHit { id, release_group }));
        }
    }
    best.map(|(_, hit)| hit)
}

fn first_release_id(body: &[u8]) -> Option<String> {
    let json: Value = serde_json::from_slice(body).ok()?;
    json.get("releases")?
        .as_array()?
        .first()?
        .get("id")?
        .as_str()
        .map(str::to_owned)
}

fn best_genre(body: &[u8]) -> Option<String> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let mut best: Option<(u64, String)> = None;
    for genre in json.get("genres")?.as_array()? {
        let name = genre.get("name")?.as_str()?.trim();
        if name.is_empty() {
            continue;
        }
        let count = json_u64(genre.get("count"));
        if best.as_ref().is_none_or(|(current, _)| count > *current) {
            best = Some((count, name.to_owned()));
        }
    }
    best.map(|(_, name)| name)
}

fn best_acoustid(body: &[u8]) -> Parsed<(Option<String>, Option<String>)> {
    let Ok(json) = serde_json::from_slice::<Value>(body) else {
        return Parsed::Retry;
    };
    if json.get("status").and_then(Value::as_str) != Some("ok") {
        return Parsed::Retry;
    }
    let Some(results) = json.get("results").and_then(Value::as_array) else {
        return Parsed::Empty;
    };
    let mut best_score = MIN_ACOUSTID_SCORE;
    let mut chosen: Option<&Value> = None;
    for result in results {
        let score = result.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        if score >= best_score {
            // Prefer a later equal score only when it is strictly higher, so
            // the first acceptable match wins ties.
            if chosen.is_none() || score > best_score {
                best_score = score;
                chosen = Some(result);
            }
        }
    }
    let Some(result) = chosen else {
        return Parsed::Empty;
    };
    let Some(recording) = result
        .get("recordings")
        .and_then(Value::as_array)
        .and_then(|recordings| recordings.first())
    else {
        return Parsed::Empty;
    };
    let release_id = recording
        .get("releases")
        .and_then(Value::as_array)
        .and_then(|releases| releases.first())
        .and_then(|release| release.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let release_group = recording
        .get("releasegroups")
        .and_then(Value::as_array)
        .and_then(|groups| groups.first())
        .and_then(|group| group.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if release_id.is_none() && release_group.is_none() {
        Parsed::Empty
    } else {
        Parsed::Value((release_id, release_group))
    }
}

fn json_u64(value: Option<&Value>) -> u64 {
    let Some(value) = value else {
        return 0;
    };
    value
        .as_u64()
        .or_else(|| value.as_f64().map(|number| number as u64))
        .unwrap_or(0)
}

fn quote_lucene(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        if ch == '\\' || ch == '"' {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    format!("\"{escaped}\"")
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

struct ReqwestHttp {
    client: reqwest::blocking::Client,
    last: Mutex<Option<Instant>>,
}

impl ReqwestHttp {
    fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .context("could not build the metadata http client")?;
        Ok(Self {
            client,
            last: Mutex::new(None),
        })
    }

    fn pace(&self) {
        let mut last = self.last.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(then) = *last {
            if let Some(rest) = REQUEST_GAP.checked_sub(then.elapsed()) {
                std::thread::sleep(rest);
            }
        }
        *last = Some(Instant::now());
    }

    fn fetched(response: reqwest::blocking::Response) -> Result<Fetched> {
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .context("could not read metadata response")?
            .to_vec();
        Ok(Fetched { status, body })
    }
}

impl Http for ReqwestHttp {
    fn get(&self, url: &str) -> Result<Fetched> {
        self.pace();
        let response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("metadata request failed for {url}"))?;
        Self::fetched(response)
    }

    fn post(&self, url: &str, content_type: &str, body: &str) -> Result<Fetched> {
        self.pace();
        let response = self
            .client
            .post(url)
            .header("content-type", content_type)
            .body(body.to_owned())
            .send()
            .with_context(|| format!("metadata request failed for {url}"))?;
        Self::fetched(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_keys_ignore_case_and_outer_space() {
        assert_eq!(
            album_key("  Radiohead ", "OK Computer"),
            album_key("radiohead", "ok computer")
        );
    }

    #[test]
    fn tags_need_both_an_artist_and_an_album() {
        assert!(tags_searchable("Radiohead", "OK Computer"));
        assert!(!tags_searchable(" ", "OK Computer"));
        assert!(!tags_searchable("Radiohead", ""));
    }

    #[test]
    fn release_search_quotes_the_names() {
        let url = release_search_url("Radiohead", "OK \"Computer\"");
        assert!(url.contains("release%3A%22OK%20%5C%22Computer%5C%22%22"));
        assert!(url.contains("artist%3A%22Radiohead%22"));
        assert!(url.starts_with("https://musicbrainz.org/ws/2/release/?query="));
    }

    #[test]
    fn a_tagged_album_uses_musicbrainz_and_cover_art_not_acoustid() {
        let http = Script::new(vec![
            (
                "https://musicbrainz.org/ws/2/release/?",
                200,
                br#"{"releases":[
                    {"id":"low","score":40,"title":"Other"},
                    {"id":"rel-1","score":100,"title":"OK Computer","release-group":{"id":"rg-1"}}
                ]}"#
                .to_vec(),
            ),
            (
                "https://musicbrainz.org/ws/2/release-group/rg-1",
                200,
                br#"{"genres":[{"name":"electronic","count":1},{"name":"alternative rock","count":8}]}"#.to_vec(),
            ),
            (
                "https://coverartarchive.org/release/rel-1/front-500",
                200,
                b"jpeg-bytes".to_vec(),
            ),
        ]);
        let outcome = resolve(
            &http,
            &Query {
                artist: "Radiohead".into(),
                album: "OK Computer".into(),
                duration_secs: 180,
                fingerprint: Some("unused".into()),
                fetch_art: true,
            },
        );
        assert_eq!(
            outcome,
            Outcome::Found {
                release_mbid: "rel-1".into(),
                genre: Some("alternative rock".into()),
                art: Some(b"jpeg-bytes".to_vec()),
            }
        );
        assert!(http.posts.lock().unwrap().is_empty());
    }

    #[test]
    fn thin_tags_use_acoustid_then_cover_art() {
        let http = Script::new(vec![(
            "https://coverartarchive.org/release/rel-9/front-500",
            200,
            b"cover".to_vec(),
        )])
        .with_post(
            "https://api.acoustid.org/v2/lookup",
            200,
            br#"{"status":"ok","results":[{"score":0.92,"recordings":[{
                "releases":[{"id":"rel-9"}],
                "releasegroups":[{"id":"rg-9"}]
            }]}]}"#
                .to_vec(),
        )
        .with_get(
            "https://musicbrainz.org/ws/2/release-group/rg-9",
            200,
            br#"{"genres":[{"name":"jazz","count":3}]}"#.to_vec(),
        );
        let outcome = resolve(
            &http,
            &Query {
                artist: String::new(),
                album: String::new(),
                duration_secs: 200,
                fingerprint: Some("fp+value".into()),
                fetch_art: true,
            },
        );
        assert_eq!(
            outcome,
            Outcome::Found {
                release_mbid: "rel-9".into(),
                genre: Some("jazz".into()),
                art: Some(b"cover".to_vec()),
            }
        );
        let posts = http.posts.lock().unwrap();
        assert_eq!(posts.len(), 1);
        assert!(posts[0].contains("fingerprint=fp%2Bvalue"));
        assert!(posts[0].contains("client=NjAjoPhUOg"));
    }

    #[test]
    fn a_musicbrainz_outage_is_retried_instead_of_cached_as_missing() {
        let http = Script::new(vec![(
            "https://musicbrainz.org/ws/2/release/?",
            503,
            b"busy".to_vec(),
        )]);
        let outcome = resolve(
            &http,
            &Query {
                artist: "A".into(),
                album: "B".into(),
                duration_secs: 10,
                fingerprint: None,
                fetch_art: true,
            },
        );
        assert_eq!(outcome, Outcome::Retry);
    }

    #[test]
    fn a_cover_miss_still_keeps_the_genre() {
        let http = Script::new(vec![
            (
                "https://musicbrainz.org/ws/2/release/?",
                200,
                br#"{"releases":[{"id":"rel","score":90,"release-group":{"id":"rg"}}]}"#.to_vec(),
            ),
            (
                "https://musicbrainz.org/ws/2/release-group/rg",
                200,
                br#"{"genres":[{"name":"rock","count":2}]}"#.to_vec(),
            ),
            (
                "https://coverartarchive.org/release/rel/front-500",
                404,
                b"".to_vec(),
            ),
            (
                "https://coverartarchive.org/release-group/rg/front-500",
                404,
                b"".to_vec(),
            ),
        ]);
        let outcome = resolve(
            &http,
            &Query {
                artist: "A".into(),
                album: "B".into(),
                duration_secs: 10,
                fingerprint: None,
                fetch_art: true,
            },
        );
        assert_eq!(
            outcome,
            Outcome::Found {
                release_mbid: "rel".into(),
                genre: Some("rock".into()),
                art: None,
            }
        );
    }

    struct Script {
        gets: Vec<(&'static str, u16, Vec<u8>)>,
        posts: Mutex<Vec<String>>,
        post_reply: Option<(&'static str, u16, Vec<u8>)>,
    }

    impl Script {
        fn new(gets: Vec<(&'static str, u16, Vec<u8>)>) -> Self {
            Self {
                gets,
                posts: Mutex::new(Vec::new()),
                post_reply: None,
            }
        }

        fn with_get(mut self, prefix: &'static str, status: u16, body: Vec<u8>) -> Self {
            self.gets.push((prefix, status, body));
            self
        }

        fn with_post(mut self, prefix: &'static str, status: u16, body: Vec<u8>) -> Self {
            self.post_reply = Some((prefix, status, body));
            self
        }
    }

    impl Http for Script {
        fn get(&self, url: &str) -> Result<Fetched> {
            self.gets
                .iter()
                .find(|(prefix, _, _)| url.starts_with(prefix))
                .map(|(_, status, body)| Fetched {
                    status: *status,
                    body: body.clone(),
                })
                .context("unexpected GET")
        }

        fn post(&self, url: &str, _content_type: &str, body: &str) -> Result<Fetched> {
            self.posts.lock().unwrap().push(body.to_owned());
            let Some((prefix, status, reply)) = &self.post_reply else {
                anyhow::bail!("unexpected POST {url}");
            };
            if !url.starts_with(prefix) {
                anyhow::bail!("unexpected POST {url}");
            }
            Ok(Fetched {
                status: *status,
                body: reply.clone(),
            })
        }
    }
}
