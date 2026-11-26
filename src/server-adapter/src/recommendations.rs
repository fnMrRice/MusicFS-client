use super::{check_id, decode, online_track::online_id, Result, ServerSession, TrackInfo};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedPlaylist {
    pub external_id: String,
    pub title: String,
    pub track_count: Option<u64>,
    pub creator: Option<String>,
    pub cover_url: Option<String>,
}
#[derive(Deserialize, Serialize)]
pub struct Recommendation {
    pub playlist: RecommendedPlaylist,
    pub reason: String,
}
#[derive(Deserialize, Serialize)]
pub struct SourceFailure {
    pub code: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub instance_id: String,
    #[serde(rename(deserialize = "name"))]
    display_name: Option<String>,
    #[serde(skip)]
    plugin: Option<Plugin>,
    #[serde(skip_serializing)]
    account_id: Option<String>,
    #[serde(skip)]
    pub can_play_online: bool,
    pub items: Vec<Recommendation>,
    pub stale: bool,
    pub last_success_at: Option<String>,
    pub error: Option<SourceFailure>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationFeed {
    pub sources: Vec<Source>,
    // 历史功能由服务端响应声明。
    #[serde(default)]
    pub history_depth: u8,
}
#[derive(Deserialize)]
struct Directory {
    items: Vec<Instance>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Instance {
    instance_id: String,
    #[serde(default)]
    availability: String,
    #[serde(default)]
    capabilities: Vec<Capability>,
    plugin: Option<Plugin>,
}

#[derive(Deserialize)]
struct Capability {
    name: String,
    supported: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Plugin {
    name: String,
    #[serde(default)]
    localized_names: HashMap<String, String>,
}

impl Source {
    pub fn label(&self, languages: &[&str]) -> Option<String> {
        let Some(plugin) = &self.plugin else {
            return self.display_name.clone();
        };
        let localized = languages
            .iter()
            .find_map(|tag| plugin.localized_names.get(*tag))
            .unwrap_or(&plugin.name);
        let account = self.display_name.as_deref().filter(|name| {
            self.account_id.as_deref().is_some_and(|id| !id.is_empty())
                && !name.is_empty()
                && *name != self.instance_id
                && *name != plugin.name
                && !plugin
                    .localized_names
                    .values()
                    .any(|translation| translation == name)
        });
        Some(match account {
            Some(name) => format!("{localized} · {name}"),
            None => localized.clone(),
        })
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedEntry {
    pub position: u64,
    pub external_track_id: String,
    #[serde(default)]
    pub track: Option<TrackInfo>,
    #[serde(default)]
    pub unavailable: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackItem {
    position: u64,
    external_id: String,
    track: Option<TrackInfo>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackPage {
    playlist: RecommendedPlaylist,
    items: Vec<TrackItem>,
    next_cursor: Option<String>,
    complete: bool,
    observed_at: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedPlaylistDetail {
    pub playlist: RecommendedPlaylist,
    pub entries: Vec<ObservedEntry>,
    pub complete: bool,
    pub observed_at: String,
    pub next_cursor: Option<String>,
}

impl RecommendationFeed {
    fn apply_directory(&mut self, directory: Directory) {
        for instance in directory.items {
            if let Some(source) = self
                .sources
                .iter_mut()
                .find(|s| s.instance_id == instance.instance_id)
            {
                source.plugin = instance.plugin;
                source.can_play_online = instance.availability == "available"
                    && instance
                        .capabilities
                        .iter()
                        .any(|c| c.name == "stream.resolve" && c.supported);
            }
        }
    }
}

impl ServerSession {
    pub async fn recommendations(&self, history: u8, refresh: bool) -> Result<RecommendationFeed> {
        if history > 5 || (refresh && history > 0) {
            return Err("Invalid recommendation history request.".into());
        }
        let mut url = self.url("recommendations");
        if history > 0 {
            url.query_pairs_mut()
                .append_pair("history", &history.to_string());
        }
        if refresh {
            url.query_pairs_mut().append_pair("refresh", "true");
        }
        let mut feed: RecommendationFeed = decode(self.send(Method::GET, url, None).await?).await?;
        if feed.history_depth > 5 {
            return Err("Invalid recommendation history depth.".into());
        }
        if !feed.sources.is_empty() {
            // 目录提供账号能力与本地化插件名，读取失败时仍可浏览已取得的推荐。
            let instances: Option<Directory> = match self
                .send(Method::GET, self.url("online-instances"), None)
                .await
            {
                Ok(response) => decode(response).await.ok(),
                Err(error) if error.contains("HTTP 401") => return Err(error),
                Err(_) => None,
            };
            let Some(instances) = instances else {
                return Ok(feed);
            };
            feed.apply_directory(instances);
        }
        Ok(feed)
    }

    pub async fn recommendation_playlist(
        &self,
        instance: &str,
        external: &str,
        cursor: Option<&str>,
    ) -> Result<RecommendedPlaylistDetail> {
        check_id(instance)?;
        online_id(external)?;
        let path = format!("online-instances/{instance}/playlists/{external}");
        let mut tracks_url = self.url(&format!("{path}/tracks"));
        tracks_url.query_pairs_mut().append_pair("limit", "50");
        if let Some(after) = cursor {
            if after.is_empty() || after.len() > 1024 {
                return Err("Invalid online playlist cursor.".into());
            }
            tracks_url.query_pairs_mut().append_pair("cursor", after);
        }
        match self.send(Method::GET, tracks_url, None).await {
            Ok(response) => {
                let page: TrackPage = decode(response).await?;
                if page.playlist.external_id != external
                    || page.items.len() > 50
                    || page
                        .next_cursor
                        .as_ref()
                        .is_some_and(|next| next.is_empty() || Some(next.as_str()) == cursor)
                    || page.items.iter().any(|item| {
                        item.track
                            .as_ref()
                            .is_some_and(|track| track.external_id != item.external_id)
                    })
                {
                    return Err("Invalid MusicFS online playlist track page.".into());
                }
                let entries = page
                    .items
                    .into_iter()
                    .map(|item| ObservedEntry {
                        position: item.position,
                        external_track_id: item.external_id,
                        unavailable: item.track.is_none(),
                        track: item.track,
                    })
                    .collect();
                return Ok(RecommendedPlaylistDetail {
                    playlist: page.playlist,
                    entries,
                    next_cursor: page.next_cursor,
                    complete: page.complete,
                    observed_at: page.observed_at,
                });
            }
            // 仅声明观察接口的来源仍能展示歌单与条目编号。
            Err(error)
                if cursor.is_none()
                    && (error.contains("HTTP 404")
                        || error.contains("HTTP 405")
                        || error.contains("UNSUPPORTED")) => {}
            Err(error) => return Err(error),
        }
        let url = self.url(&path);
        let details: RecommendedPlaylistDetail =
            decode(self.send(Method::GET, url, None).await?).await?;
        if details.playlist.external_id != external || details.entries.len() > 10_000 {
            return Err("Invalid MusicFS online playlist observation.".into());
        }
        Ok(details)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ordinary_playlist_cards_keep_their_cover_and_receive_instance_playback_capability() {
        let cover = "https://p2.music.126.net/cover.jpg";
        let mut feed: RecommendationFeed = serde_json::from_value(json!({"sources":[{
            "instanceId":"netease","name":"Listener","accountId":"user","stale":false,
            "items":[{"playlist":{"externalId":"today","title":"每日推荐","coverUrl":cover},"reason":""}]
        }]})).unwrap();
        let directory = |available| {
            serde_json::from_value(json!({"items":[{
                "instanceId":"netease","availability":available,
                "capabilities":[{"name":"stream.resolve","supported":true}]
            }]}))
            .unwrap()
        };
        feed.apply_directory(directory("available"));
        let source = &feed.sources[0];
        assert!(source.can_play_online);
        assert_eq!(source.items.len(), 1);
        assert_eq!(source.items[0].playlist.cover_url.as_deref(), Some(cover));
        feed.apply_directory(directory("loginRequired"));
        assert!(!feed.sources[0].can_play_online);
    }
}
