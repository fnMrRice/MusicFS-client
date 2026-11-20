use futures_util::{stream, StreamExt, TryStreamExt};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use url::Url;

use super::{check_id, decode, Result, ServerSession};

#[path = "categories.rs"]
mod categories;
pub use categories::{Category, CategoryKind};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub source_track_id: Option<String>,
    pub forced_source_track_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub cover_object_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    #[serde(rename(deserialize = "playlistId"))]
    pub id: String,
    pub name: String,
    pub snapshot_revision: u64,
    pub rule_revision: u64,
    pub stale: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> { pub items: Vec<T>, pub next_cursor: Option<String> }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePage<T> { items: Vec<T>, #[serde(alias = "nextAfter")] next_cursor: Option<String> }
#[derive(Deserialize)]
struct Name { name: String }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Source {
    source_track_id: String,
    availability: String,
    duration_ms: Option<u64>,
    cover_object_id: Option<String>,
    #[serde(default)]
    versions: Vec<Source>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MusicTrack {
    track_id: String,
    title: String,
    artists: Vec<Name>,
    albums: Vec<Name>,
    cover_path: Option<String>,
    selected_source: Option<Source>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollectionEntry { music_track: MusicTrack, forced_source_track_id: Option<String> }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference { track_id: Option<String>, source_track_id: Option<String>, forced_source_track_id: Option<String>, remote_source_id: Option<String> }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry { entry_id: String, source: Reference }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot { items: Vec<Entry>, next_offset: Option<u64>, revision: u64, playlist_id: String }

impl MusicTrack {
    fn into_track(self, forced: Option<String>) -> Track {
        let source = self.selected_source.as_ref().and_then(|group| match forced.as_deref() {
            Some(id) if group.source_track_id != id => group.versions.iter().find(|s| s.source_track_id == id),
            _ => Some(group),
        });
        let albums = self.albums.into_iter().map(|v| v.name).collect::<Vec<_>>().join(" / ");
        let cover_object_id = self.cover_path.as_deref().and_then(|p| p.strip_prefix("/api/v1/media/"))
            .and_then(|id| check_id(id).ok()).map(str::to_owned).or_else(|| source.and_then(|s| s.cover_object_id.clone()));
        Track { id: self.track_id, title: self.title, artists: self.artists.into_iter().map(|v| v.name).collect(),
            album: (!albums.is_empty()).then_some(albums), forced_source_track_id: forced,
            source_track_id: source.filter(|s| s.availability == "available").map(|s| s.source_track_id.clone()),
            duration_ms: source.and_then(|s| s.duration_ms), cover_object_id }
    }
}

impl ServerSession {
    pub(crate) async fn playback_source(&self, track_id: &str, forced: Option<&str>) -> Result<String> {
        let track: MusicTrack = self.get(self.url(&format!("library/tracks/{}", check_id(track_id)?))).await?;
        if track.track_id != track_id { return Err("Invalid MusicFS track response.".into()); }
        track.into_track(forced.map(str::to_owned)).source_track_id.ok_or_else(|| "No playable source for this track.".into())
    }

    fn page_url(&self, path: &str, cursor_key: &str, cursor: Option<&str>) -> Url {
        let mut url = self.url(path);
        url.query_pairs_mut().append_pair("limit", "50");
        if let Some(cursor) = cursor { url.query_pairs_mut().append_pair(cursor_key, cursor); }
        url
    }

    async fn get<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        decode(self.send(Method::GET, url, None).await?).await
    }

    pub async fn tracks(&self, query: &str, cursor: Option<&str>) -> Result<Page<Track>> {
        let mut url = self.page_url("library/tracks", "after", cursor);
        if !query.trim().is_empty() { url.query_pairs_mut().append_pair("q", query.trim()); }
        self.track_page(url).await
    }

    async fn track_page(&self, url: Url) -> Result<Page<Track>> {
        let page: WirePage<MusicTrack> = self.get(url).await?;
        Ok(Page { items: page.items.into_iter().map(|t| t.into_track(None)).collect(), next_cursor: page.next_cursor })
    }

    async fn collection(&self, path: &str, cursor: Option<&str>) -> Result<Page<Track>> {
        let page: WirePage<CollectionEntry> = self.get(self.page_url(path, "cursor", cursor)).await?;
        Ok(Page { items: page.items.into_iter().map(|e| e.music_track.into_track(e.forced_source_track_id)).collect(), next_cursor: page.next_cursor })
    }

    pub async fn favorites(&self, cursor: Option<&str>) -> Result<Page<Track>> {
        self.collection("me/favorites", cursor).await
    }

    pub async fn history(&self, cursor: Option<&str>) -> Result<Page<Track>> {
        self.collection("me/recently-played", cursor).await
    }

    pub async fn set_favorite(&self, id: &str, value: bool) -> Result<()> {
        let url = self.url(&format!("me/favorites/{}", check_id(id)?));
        self.send(if value { Method::PUT } else { Method::DELETE }, url, None).await.map(|_| ())
    }

    pub async fn playlists(&self, cursor: Option<&str>) -> Result<Page<Playlist>> {
        let page: WirePage<Playlist> = self.get(self.page_url("playlists", "after", cursor)).await?;
        Ok(Page { items: page.items, next_cursor: page.next_cursor })
    }

    async fn entry(&self, entry: Entry) -> Result<Track> {
        let r = entry.source;
        let target = if let Some(id) = &r.track_id {
            Some(self.url(&format!("library/tracks/{}", check_id(id)?)))
        } else if let Some(id) = &r.source_track_id {
            Some(self.url(&format!("library/sources/{}/track", check_id(id)?)))
        } else { None };
        let forced = r.forced_source_track_id.or_else(|| r.track_id.is_none().then(|| r.source_track_id.clone()).flatten());
        if let Some(url) = target {
            self.ensure_active()?;
            let reply = self.client.get(url).timeout(super::API_TIMEOUT).send().await.map_err(|_| "Unable to read playlist entry.")?;
            self.ensure_active()?;
            if !matches!(reply.status().as_u16(), 403 | 404) {
                return Ok(decode::<MusicTrack>(reply).await?.into_track(forced));
            }
        }
        let id = r.track_id.or(r.source_track_id).or(r.remote_source_id).unwrap_or(entry.entry_id);
        Ok(Track { title: id.clone(), id, artists: Vec::new(), album: None, source_track_id: None,
            forced_source_track_id: forced, duration_ms: None, cover_object_id: None })
    }

    pub async fn playlist_tracks(&self, id: &str, revision: u64, cursor: Option<&str>) -> Result<Page<Track>> {
        check_id(id)?;
        if revision == 0 { return Ok(Page { items: Vec::new(), next_cursor: None }); }
        let offset: u64 = cursor.unwrap_or("0").parse().map_err(|_| "Invalid playlist offset.")?;
        let mut url = self.url(&format!("playlists/{id}/snapshots/{revision}"));
        url.query_pairs_mut().append_pair("limit", "50").append_pair("offset", &offset.to_string());
        let snapshot: Snapshot = self.get(url).await?;
        if snapshot.revision != revision || snapshot.playlist_id != id || snapshot.next_offset.is_some_and(|next| next <= offset) {
            return Err("Invalid playlist snapshot.".into());
        }
        let items = stream::iter(snapshot.items).map(|entry| self.entry(entry)).buffered(4).try_collect().await?;
        Ok(Page { items, next_cursor: snapshot.next_offset.map(|n| n.to_string()) })
    }
}
