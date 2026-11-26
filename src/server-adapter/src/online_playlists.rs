pub use super::recommendations::RecommendedPlaylist as OnlinePlaylist;
use super::{check_id, decode, http_client, online_track::online_id, Page, Result, ServerSession};
use reqwest::Method;
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashMap, time::Duration};

#[derive(Deserialize)]
pub struct Directory {
    pub items: Vec<OnlineSource>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineSource {
    pub instance_id: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub availability: String,
    #[serde(default)]
    pub authentication: String,
    pub account_id: Option<String>,
    pub account_revision: Option<String>,
    pub name: Option<String>,
    pub(crate) plugin: Option<Plugin>,
    #[serde(default)]
    capabilities: Vec<Capability>,
}
#[derive(Clone, Deserialize)]
struct Capability {
    name: String,
    supported: bool,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Plugin {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) localized_names: HashMap<String, String>,
    #[serde(default)]
    features: Vec<String>,
}

impl OnlineSource {
    pub fn is_music_source(&self) -> bool {
        self.supports("playlist.list")
            || self.supports("playlist.observe")
            || self
                .plugin
                .as_ref()
                .is_some_and(|p| p.features.iter().any(|f| f == "music-source"))
    }
    pub fn label(&self, languages: &[&str]) -> String {
        let Some(plugin) = &self.plugin else {
            return self
                .name
                .clone()
                .unwrap_or_else(|| self.instance_id.clone());
        };
        let label = languages
            .iter()
            .find_map(|t| plugin.localized_names.get(*t))
            .unwrap_or(&plugin.name);
        match self.name.as_deref().filter(|n| {
            !n.is_empty()
                && *n != self.instance_id
                && *n != plugin.name
                && !plugin.localized_names.values().any(|s| s == n)
        }) {
            Some(name) if self.account_id.is_some() => format!("{label} · {name}"),
            _ => label.clone(),
        }
    }
    pub fn imported_label(&self, languages: &[&str], account: &str) -> String {
        if self.account_id.as_deref().unwrap_or_default() == account {
            return self.label(languages);
        }
        let platform = self
            .plugin
            .as_ref()
            .map(|p| {
                languages
                    .iter()
                    .find_map(|t| p.localized_names.get(*t))
                    .unwrap_or(&p.name)
                    .as_str()
            })
            .unwrap_or(&self.instance_id);
        if account.is_empty() {
            platform.to_owned()
        } else {
            format!("{platform} · {account}")
        }
    }
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.name == capability && c.supported)
    }
    pub fn available(&self, capability: &str) -> bool {
        self.availability == "available" && self.supports(capability)
    }
    pub fn import_account(&self) -> Option<&str> {
        if self.authentication == "anonymous" {
            Some("")
        } else {
            self.account_id.as_deref().filter(|a| !a.is_empty())
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedPlaylist {
    pub playlist_id: String,
    pub instance_id: String,
    pub external_id: String,
    pub account_id: String,
    pub title: String,
    pub binding_revision: u64,
    pub stale: bool,
    pub last_success_at: Option<String>,
    pub job_id: Option<String>,
    pub error: Option<PlaylistError>,
}
#[derive(Clone, Deserialize)]
pub struct PlaylistError {
    pub code: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistLink {
    pub instance_id: String,
    pub external_id: String,
    pub account_id: String,
    pub playlist_id: String,
    pub pending_count: u64,
    pub last_error_code: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncJob {
    pub job_id: String,
    pub state: String,
    pub error: Option<PlaylistError>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePage<T> {
    items: Vec<T>,
    #[serde(alias = "nextAfter")]
    next_cursor: Option<String>,
}
impl<T> From<WirePage<T>> for Page<T> {
    fn from(page: WirePage<T>) -> Self {
        Self {
            items: page.items,
            next_cursor: page.next_cursor,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    job_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_labels_follow_the_bound_account() {
        let source: OnlineSource = serde_json::from_value(json!({
            "instanceId": "netease", "name":"Listener", "accountId":"current",
            "availability":"available", "authentication":"account",
            "plugin":{"name":"NetEase","localizedNames":{"zh-CN":"网易云音乐"}},
            "capabilities":[{"name":"playlist.observe","supported":true}]
        }))
        .unwrap();
        assert!(source.available("playlist.observe"));
        assert_eq!(source.import_account(), Some("current"));
        assert_eq!(
            source.imported_label(&["zh-CN"], "current"),
            "网易云音乐 · Listener"
        );
        assert_eq!(
            source.imported_label(&["zh-CN"], "previous"),
            "网易云音乐 · previous"
        );
        assert_eq!(source.imported_label(&["zh-CN"], ""), "网易云音乐");
    }

    #[test]
    fn only_anonymous_sources_can_import_without_an_account() {
        let source = |authentication| {
            serde_json::from_value::<OnlineSource>(
                json!({"instanceId":"source","authentication":authentication}),
            )
            .unwrap()
        };
        assert_eq!(source("anonymous").import_account(), Some(""));
        assert_eq!(source("platform").import_account(), None);
    }
}

impl ServerSession {
    pub async fn playlist_sources(&self) -> Result<Directory> {
        decode(
            self.send(Method::GET, self.url("online-instances"), None)
                .await?,
        )
        .await
    }
    pub async fn account_playlists(
        &self,
        source: &OnlineSource,
        cursor: Option<&str>,
        refresh: bool,
    ) -> Result<Page<OnlinePlaylist>> {
        let id = check_id(&source.instance_id)?;
        let mut url = self.url(&format!("online-instances/{id}/playlists"));
        url.query_pairs_mut().append_pair("limit", "30");
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("cursor", cursor);
        } else if refresh {
            url.query_pairs_mut().append_pair("refresh", "true");
        }
        let response = if cursor.is_none() && source.platform == "netease" {
            let timeout = Duration::from_secs(300);
            self.send_json(
                &http_client(self.cookies.clone(), timeout)?,
                timeout,
                Method::GET,
                url,
                None,
                &[],
            )
            .await?
        } else {
            self.send(Method::GET, url, None).await?
        };
        Ok(decode::<WirePage<OnlinePlaylist>>(response).await?.into())
    }
    pub async fn imported_playlists(&self, after: Option<&str>) -> Result<Page<ImportedPlaylist>> {
        let mut url = self.url("remote-playlists");
        url.query_pairs_mut().append_pair("limit", "100");
        if let Some(after) = after {
            url.query_pairs_mut().append_pair("after", after);
        }
        Ok(
            decode::<WirePage<ImportedPlaylist>>(self.send(Method::GET, url, None).await?)
                .await?
                .into(),
        )
    }
    pub async fn playlist_reference(&self, source: &OnlineSource, input: &str) -> Result<String> {
        if source.platform != "bilibili" {
            return Ok(online_id(input)?.to_owned());
        }
        #[derive(Deserialize)]
        struct Search {
            playlists: Vec<OnlinePlaylist>,
        }
        let instance = check_id(&source.instance_id)?;
        let mut url = self.url(&format!("online-instances/{instance}/search"));
        url.query_pairs_mut()
            .append_pair("kind", "playlists")
            .append_pair("q", input)
            .append_pair("limit", "1");
        let found: Search = decode(self.send(Method::GET, url, None).await?).await?;
        let item = found
            .playlists
            .first()
            .ok_or("Playlist reference was not found.")?;
        Ok(online_id(&item.external_id)?.to_owned())
    }
    pub async fn import_playlist(
        &self,
        source: &OnlineSource,
        external: &str,
        key: &str,
    ) -> Result<String> {
        let account = source
            .import_account()
            .ok_or("Sign in to the platform account.")?;
        self.playlist_job_request("remote-playlists",json!({"instanceId":check_id(&source.instance_id)?,"externalId":online_id(external)?,"accountId":account}),key).await
    }
    pub async fn sync_playlist(&self, playlist: &ImportedPlaylist, key: &str) -> Result<String> {
        let id = check_id(&playlist.playlist_id)?;
        self.playlist_job_request(
            &format!("remote-playlists/{id}/sync-jobs"),
            json!({"expectedBindingRevision":playlist.binding_revision}),
            key,
        )
        .await
    }
    async fn playlist_job_request(
        &self,
        path: &str,
        body: serde_json::Value,
        key: &str,
    ) -> Result<String> {
        let response = self
            .send_with_headers(
                Method::POST,
                self.url(path),
                Some(&body),
                &[("Idempotency-Key", key)],
            )
            .await?;
        Ok(check_id(&decode::<Receipt>(response).await?.job_id)?.to_owned())
    }
    pub async fn playlist_job(&self, id: &str) -> Result<SyncJob> {
        let id = check_id(id)?;
        let job: SyncJob = decode(
            self.send(
                Method::GET,
                self.url(&format!("remote-playlist-sync-jobs/{id}")),
                None,
            )
            .await?,
        )
        .await?;
        if job.job_id != id {
            return Err("Unexpected playlist synchronization job.".into());
        }
        Ok(job)
    }
    pub async fn playlist_links(&self) -> Result<Vec<PlaylistLink>> {
        #[derive(Deserialize)]
        struct Links {
            items: Vec<PlaylistLink>,
        }
        Ok(decode::<Links>(
            self.send(Method::GET, self.url("playlist-links"), None)
                .await?,
        )
        .await?
        .items)
    }
    pub async fn change_playlist_link(
        &self,
        source: &OnlineSource,
        external: &str,
        target: &str,
        remove: bool,
    ) -> Result<()> {
        let instance = check_id(&source.instance_id)?;
        let external = online_id(external)?;
        let body = json!({"accountId":source.account_id.as_deref().ok_or("Sign in to the platform account.")?, "playlistId":check_id(target)?});
        self.send(
            if remove { Method::DELETE } else { Method::PUT },
            self.url(&format!(
                "online-instances/{instance}/playlists/{external}/link"
            )),
            Some(&body),
        )
        .await?;
        Ok(())
    }
}
