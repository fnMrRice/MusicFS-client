use crate::{check_id, decode, Result, ServerSession};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize};
use std::sync::Arc;

#[cfg(test)]
#[path = "../../../tests/native/folders.rs"]
mod tests;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub category_id: String,
    kind: String,
    pub name: String,
    pub provider_instance_id: String,
    pub root_id: String,
    pub parent_id: Option<String>,
    pub track_count: u64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub source_track_id: String,
    pub provider_instance_id: String,
    pub display_name: String,
    pub title: Option<String>,
    pub artists: Option<Vec<String>>,
    pub album: Option<String>,
    pub cover_object_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub availability: String,
}

#[derive(Clone)]
pub struct Cursor {
    snapshot: Arc<Snapshot>,
    pub token: String,
}

struct Snapshot {
    server: ServerSession,
    id: String,
}

// 最后一页或用户离开页面后，释放不可变查询；分页中的并发任务共享所有权。
impl Drop for Snapshot {
    fn drop(&mut self) {
        let s = self.server.clone();
        let path = format!("library/queries/{}", self.id);
        self.server.runtime.spawn(async move {
            let _ = s.send(Method::DELETE, s.url(&path), None).await;
        });
    }
}

pub struct FolderPage<T> {
    pub items: Vec<T>,
    pub next: Option<Cursor>,
    pub total: usize,
}

#[derive(Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Scope {
    #[serde(rename = "type")]
    kind: String,
    category_kind: String,
    provider_instance_id: String,
    category_id: String,
    parent_id: String,
    q: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePage<T> {
    query_id: String,
    query: Scope,
    items: Vec<T>,
    total: usize,
    next_cursor: Option<String>,
}

impl ServerSession {
    pub async fn folders(
        &self,
        provider: Option<&str>,
        parent: Option<&str>,
        search: &str,
        cursor: Option<Cursor>,
    ) -> Result<FolderPage<Folder>> {
        let page: FolderPage<Folder> = self
            .folder_page(provider, parent, search, cursor, false)
            .await?;
        if page.items.iter().any(|f| {
            f.kind != "folder"
                || f.category_id.is_empty()
                || f.provider_instance_id.is_empty()
                || provider.is_some_and(|p| p != f.provider_instance_id)
                || (search.is_empty() && f.parent_id.as_deref() != parent)
        }) {
            return Err("Invalid MusicFS folder response.".into());
        }
        Ok(page)
    }
    pub async fn folder_recordings(
        &self,
        provider: Option<&str>,
        folder: &str,
        search: &str,
        cursor: Option<Cursor>,
    ) -> Result<FolderPage<Recording>> {
        let page: FolderPage<Recording> = self
            .folder_page(provider, Some(folder), search, cursor, true)
            .await?;
        if page.items.iter().any(|t| {
            check_id(&t.source_track_id).is_err()
                || t.provider_instance_id.is_empty()
                || provider.is_some_and(|p| p != t.provider_instance_id)
        }) {
            return Err("Invalid MusicFS source track.".into());
        }
        Ok(page)
    }
    async fn folder_page<T: DeserializeOwned>(
        &self,
        provider: Option<&str>,
        folder: Option<&str>,
        search: &str,
        cursor: Option<Cursor>,
        tracks: bool,
    ) -> Result<FolderPage<T>> {
        if cursor
            .as_ref()
            .is_some_and(|c| !Arc::ptr_eq(&self.active, &c.snapshot.server.active))
        {
            return Err("Mismatched MusicFS catalog session.".into());
        }
        let kind = if tracks { "tracks" } else { "categories" };
        let parent = folder.unwrap_or_default();
        let mut url = self.url(if tracks {
            "library/search"
        } else {
            "library/categories"
        });
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("categoryKind", "folder")
                .append_pair("limit", "100");
            if tracks {
                q.append_pair("type", "tracks");
            }
            if let Some(p) = provider {
                q.append_pair("providerInstanceId", p);
            }
            if !parent.is_empty() {
                q.append_pair(if tracks { "categoryId" } else { "parentId" }, parent);
            }
            if !search.trim().is_empty() {
                q.append_pair("q", search.trim());
            }
            if let Some(c) = &cursor {
                q.append_pair("cursor", &c.token);
            }
        }
        let page: WirePage<T> = decode(self.send(Method::GET, url, None).await?).await?;
        check_id(&page.query_id)?;
        let snapshot = match cursor {
            Some(c) if c.snapshot.id == page.query_id => c.snapshot,
            Some(_) => return Err("Catalog page belongs to a different snapshot.".into()),
            None => Arc::new(Snapshot {
                server: self.clone(),
                id: page.query_id,
            }),
        };
        let scope = page.query;
        if scope.kind != kind
            || scope.category_kind != "folder"
            || scope.provider_instance_id != provider.unwrap_or_default()
            || scope.q != search.trim()
            || scope.category_id != if tracks { parent } else { "" }
            || scope.parent_id != if tracks { "" } else { parent }
            || page.next_cursor.as_ref().is_some_and(String::is_empty)
        {
            return Err("Catalog scope does not match folder query.".into());
        }
        Ok(FolderPage {
            items: page.items,
            total: page.total,
            next: page.next_cursor.map(|token| Cursor { snapshot, token }),
        })
    }
}
