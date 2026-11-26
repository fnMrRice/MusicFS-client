use reqwest::{header, Method};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

use super::{check_id, decode, http_client, limited, Result, ServerSession};

#[derive(Deserialize)]
struct Selection {
    target: Target,
    revision: u64,
    bindings: Vec<Binding>,
}
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Target {
    source_track_id: String,
    asset_id: String,
    asset_version: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Binding {
    field: String,
    state: String,
    current: bool,
    object_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsCandidate {
    candidate_token: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub lyrics: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidates {
    plugin_id: String,
    items: Vec<LyricsCandidate>,
}

pub struct LyricsMatches {
    selection: Selection,
    candidates: Candidates,
}

impl LyricsMatches {
    pub fn items(&self) -> &[LyricsCandidate] {
        &self.candidates.items
    }

    pub async fn apply(&self, server: &ServerSession, index: usize, key: &str) -> Result<()> {
        let item = self.items().get(index).ok_or("Unknown lyrics candidate.")?;
        let target = &self.selection.target;
        let body = json!({
            "expectedRevision": self.selection.revision,
            "assetId": target.asset_id,
            "assetVersion": target.asset_version,
            "pluginId": self.candidates.plugin_id,
            "candidateToken": item.candidate_token,
        });
        let url = server.url(&format!(
            "source-tracks/{}/scraped-fields/lyrics",
            target.source_track_id
        ));
        let response = server
            .send_with_headers(Method::PUT, url, Some(&body), &[("Idempotency-Key", key)])
            .await?;
        let saved: Selection = decode(response).await?;
        if saved.target != *target || saved.revision <= self.selection.revision {
            return Err("Unexpected lyrics selection.".into());
        }
        Ok(())
    }
}

impl ServerSession {
    async fn lyrics_selection(&self, source_track_id: &str) -> Result<Selection> {
        let id = check_id(source_track_id)?;
        let url = self.url(&format!("source-tracks/{id}/media-selection"));
        let selection: Selection = decode(self.send(Method::GET, url, None).await?).await?;
        if selection.target.source_track_id != id
            || selection.target.asset_id.is_empty()
            || selection.target.asset_version.is_empty()
        {
            return Err("Invalid track media selection.".into());
        }
        Ok(selection)
    }

    pub async fn search_lyrics(
        &self,
        source_track_id: &str,
        title: &str,
        artist: &str,
        duration_ms: Option<u64>,
    ) -> Result<LyricsMatches> {
        // 保留查找时的资产版本与修订，避免覆盖查找期间其他设备选用的歌词。
        let selection = self.lyrics_selection(source_track_id).await?;
        let mut query = json!({"title": title, "artist": artist});
        if let Some(ms) = duration_ms {
            query["duration"] = json!(ms as f64 / 1000.);
        }
        let body = json!({"sourceTrackId": source_track_id, "query": query});
        // 服务端可能依次查询多个歌词源，沿用网页端的两分钟查询时限。
        let timeout = Duration::from_secs(120);
        let client = http_client(self.cookies.clone(), timeout)?;
        let response = self
            .send_json(
                &client,
                timeout,
                Method::POST,
                self.url("scrapers/lyrics-auto-search"),
                Some(&body),
                &[],
            )
            .await?;
        let candidates: Candidates = decode(response).await?;
        Ok(LyricsMatches {
            selection,
            candidates,
        })
    }

    pub async fn lyrics(&self, source_track_id: &str) -> Result<Option<String>> {
        let selection = self.lyrics_selection(source_track_id).await?;
        let Some(binding) = selection
            .bindings
            .into_iter()
            .find(|b| b.current && b.state == "available" && b.field == "lyrics")
        else {
            return Ok(None);
        };
        let url = self.url(&format!("media/{}", check_id(&binding.object_id)?));
        let response = self.send(Method::GET, url, None).await?;
        let mime = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next());
        if !mime.is_some_and(|s| s.trim().eq_ignore_ascii_case("text/plain")) {
            return Err("Unsupported lyrics format.".into());
        }
        let lyrics = String::from_utf8(limited(response, 1024 * 1024).await?)
            .map_err(|_| "Lyrics are not valid UTF-8.")?;
        Ok((!lyrics.trim().is_empty()).then_some(lyrics))
    }
}
