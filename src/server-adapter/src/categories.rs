use super::{Page, Result, ServerSession, Track, WirePage};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CategoryKind { Artist, Album, Genre }

impl CategoryKind {
    fn key(self) -> &'static str {
        match self { Self::Artist => "artist", Self::Album => "album", Self::Genre => "genre" }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    artist_id: Option<String>,
    album_id: Option<String>,
    name: String,
    track_count: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category { pub id: String, pub name: String, pub track_count: u64 }

impl ServerSession {
    pub async fn categories(&self, kind: CategoryKind, cursor: Option<&str>) -> Result<Page<Category>> {
        let url = self.page_url(&format!("library/{}s", kind.key()), "after", cursor);
        let page: WirePage<Entry> = self.get(url).await?;
        let items = page.items.into_iter().map(|item| {
            let id = match kind {
                CategoryKind::Artist => item.artist_id,
                CategoryKind::Album => item.album_id,
                CategoryKind::Genre => Some(item.name.clone()),
            }.ok_or("Invalid MusicFS category.")?;
            Ok(Category { id, name: item.name, track_count: item.track_count })
        }).collect::<Result<Vec<_>>>()?;
        Ok(Page { items, next_cursor: page.next_cursor })
    }

    pub async fn category_tracks(&self, kind: CategoryKind, id: &str, cursor: Option<&str>) -> Result<Page<Track>> {
        let mut url = self.page_url("library/tracks", "after", cursor);
        let key = match kind {
            CategoryKind::Artist => "artistId", CategoryKind::Album => "albumId", CategoryKind::Genre => "genre",
        };
        if id.is_empty() { url.query_pairs_mut().append_pair("unknown", kind.key()); }
        else { url.query_pairs_mut().append_pair(key, id); }
        self.track_page(url).await
    }
}
