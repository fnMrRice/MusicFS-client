use super::{Page, Result, ServerSession, Track, WirePage};
use serde::{Deserialize, Serialize};

fn local_album_cover(path: Option<String>) -> Option<String> {
    path.filter(|p| p.starts_with("/api/v1/") && crate::covers::cover_endpoint(p).is_ok())
}

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
    pub cover_path: Option<String>,
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
                    cover_path: local_album_cover(item.cover_path),
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

    pub async fn album_cover_path(&self, id: &str) -> Result<Option<String>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct AlbumCover {
            cover_path: Option<String>,
        }
        let id = super::check_id(id)?;
        let album: AlbumCover = self.get(self.url(&format!("library/albums/{id}"))).await?;
        Ok(local_album_cover(album.cover_path))
    }

    pub async fn refresh_album_cover(&self, id: &str) -> Result<()> {
        let id = super::check_id(id)?;
        let key = uuid::Uuid::new_v4().to_string();
        self.request_album_artwork(
            reqwest::Method::POST,
            &format!("library/albums/{id}/artwork/refresh"),
            &[("Idempotency-Key", &key)],
        )
        .await?;
        Ok(())
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
