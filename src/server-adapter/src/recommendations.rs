use super::{check_id, decode, Result, ServerSession};
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
    pub display_name: Option<String>,
    #[serde(skip_serializing)]
    account_id: Option<String>,
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
    plugin: Option<Plugin>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Plugin {
    name: String,
    #[serde(default)]
    localized_names: HashMap<String, String>,
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
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub external_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: Option<u64>,
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
            // 目录只补充本地化插件名，读取失败时保留推荐页已提供的来源显示名称。
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
            for source in &mut feed.sources {
                if let Some(plugin) = instances
                    .items
                    .iter()
                    .find(|i| i.instance_id == source.instance_id)
                    .and_then(|i| i.plugin.as_ref())
                {
                    let localized = ["zh-CN", "zh-Hans", "zh"]
                        .iter()
                        .find_map(|tag| plugin.localized_names.get(*tag))
                        .unwrap_or(&plugin.name);
                    let account_name = source.display_name.as_deref().filter(|name| {
                        source
                            .account_id
                            .as_deref()
                            .is_some_and(|account| !account.is_empty())
                            && !name.is_empty()
                            && *name != source.instance_id
                            && *name != plugin.name
                            && *name != localized
                    });
                    source.display_name = Some(match account_name {
                        Some(name) => format!("{localized} · {name}"),
                        None => localized.clone(),
                    });
                }
            }
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
        let bytes = external.as_bytes();
        if bytes.len() > 128
            || !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
            || !bytes
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || b":_.-".contains(c))
        {
            return Err("Invalid online playlist identifier.".into());
        }
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
