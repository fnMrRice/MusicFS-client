use super::{Page, Result, ServerSession, Track, WirePage};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CategoryKind {
    Artist,
    Album,
    Genre,
}

impl CategoryKind {
    fn key(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Genre => "genre",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    artist_id: Option<String>,
    album_id: Option<String>,
    name: String,
    track_count: u64,
    cover_path: Option<String>,
    publisher: Option<String>,
    release_date: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub track_count: u64,
    pub cover_object_id: Option<String>,
    pub publisher: Option<String>,
    pub release_date: Option<String>,
}

impl ServerSession {
    pub async fn categories(
        &self,
        kind: CategoryKind,
        provider: Option<&str>,
        query: &str,
        cursor: Option<&str>,
    ) -> Result<Page<Category>> {
        self.filtered_categories(kind, provider, query, cursor, false)
            .await
    }

    pub async fn filtered_categories(
        &self,
        kind: CategoryKind,
        provider: Option<&str>,
        query: &str,
        cursor: Option<&str>,
        unscraped_only: bool,
    ) -> Result<Page<Category>> {
        let mut url =
            self.catalog_url(&format!("library/{}s", kind.key()), provider, query, cursor);
        if matches!(kind, CategoryKind::Album) && unscraped_only {
            url.query_pairs_mut().append_pair("unscrapedOnly", "true");
        }
        let page: WirePage<Entry> = self.get(url).await?;
        let items = page
            .items
            .into_iter()
            .map(|item| {
                let id = match kind {
                    CategoryKind::Artist => item.artist_id,
                    CategoryKind::Album => item.album_id,
                    CategoryKind::Genre => Some(item.name.clone()),
                }
                .ok_or("Invalid MusicFS category.")?;
                Ok(Category {
                    id,
                    name: item.name,
                    track_count: item.track_count,
                    cover_object_id: item
                        .cover_path
                        .as_deref()
                        .and_then(|p| p.strip_prefix("/api/v1/media/"))
                        .and_then(|id| super::check_id(id).ok())
                        .map(str::to_owned),
                    publisher: item.publisher,
                    release_date: item.release_date,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Page {
            items,
            next_cursor: page.next_cursor,
        })
    }

    pub async fn category_tracks(
        &self,
        kind: CategoryKind,
        id: &str,
        provider: Option<&str>,
        query: &str,
        cursor: Option<&str>,
    ) -> Result<Page<Track>> {
        let mut url = self.catalog_url("library/tracks", provider, query, cursor);
        let key = match kind {
            CategoryKind::Artist => "artistId",
            CategoryKind::Album => "albumId",
            CategoryKind::Genre => "genre",
        };
        if id.is_empty() {
            url.query_pairs_mut().append_pair("unknown", kind.key());
        } else {
            url.query_pairs_mut().append_pair(key, id);
        }
        self.track_page(url).await
    }
}
