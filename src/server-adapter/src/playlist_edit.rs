use super::{check_id, decode, Result, ServerSession, API_TIMEOUT};
use reqwest::header;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistChange {
    pub playlist_id: String,
    pub rule_revision: u64,
    pub count: usize,
}

fn selection(source_ids: &[String]) -> Result<Value> {
    if source_ids.is_empty() || source_ids.len() > 10_000 {
        return Err("Select between one and 10000 source tracks.".into());
    }
    let mut ids = HashSet::with_capacity(source_ids.len());
    for id in source_ids {
        check_id(id)?;
        if !ids.insert(id.as_str()) {
            return Err("Source track selection contains duplicates.".into());
        }
    }
    Ok(json!({"batch":false,"sourceTrackIds":source_ids}))
}

impl ServerSession {
    pub async fn playlist_rules(&self, id: &str) -> Result<(super::Playlist, Vec<Value>)> {
        let path = format!("/playlists/{}", check_id(id)?);
        let mut value = self.settings_json("GET", &path, None, &[]).await?;
        let bindings = value["bindings"]
            .as_array_mut()
            .map(std::mem::take)
            .ok_or("Invalid playlist bindings.")?;
        let playlist: super::Playlist =
            serde_json::from_value(value).map_err(|_| "Invalid playlist response.")?;
        if playlist.id != id {
            return Err("Invalid playlist response.".into());
        }
        Ok((playlist, bindings))
    }

    pub async fn save_playlist_rules(
        &self,
        target: Option<(&str, u64)>,
        name: &str,
        bindings: &[Value],
        key: &str,
    ) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 256 || name.chars().any(char::is_control) {
            return Err("Enter a playlist name of at most 256 characters.".into());
        }
        let mut body = json!({"name":name, "bindings":bindings});
        if let Some((_, revision)) = target {
            body["expectedRevision"] = json!(revision);
        }
        self.submit_track_references(target.map(|(id, _)| id), &body, key)
            .await
    }

    async fn playlist_write<T: DeserializeOwned>(
        &self,
        path: &str,
        body: Value,
        key: &str,
    ) -> Result<T> {
        if !(8..=128).contains(&key.len()) || !key.bytes().all(|c| c.is_ascii_graphic()) {
            return Err("Invalid playlist operation key.".into());
        }
        // 超时可能发生在事务提交之后，仅用同一幂等键重试一次。
        for attempt in 0..2 {
            self.ensure_active()?;
            let response = self
                .client
                .post(self.url(path))
                .header(header::ORIGIN, &self.info.endpoint)
                .header("X-CSRF-Token", &self.csrf)
                .header("Idempotency-Key", key)
                .json(&body)
                .timeout(API_TIMEOUT)
                .send()
                .await;
            match response {
                Ok(reply) => return decode(reply).await,
                Err(_) if attempt == 0 => continue,
                Err(_) => return Err("Unable to confirm the playlist operation. Refresh the playlist before retrying.".into()),
            }
        }
        unreachable!()
    }

    pub async fn create_playlist(
        &self,
        name: &str,
        source_ids: &[String],
        key: &str,
    ) -> Result<PlaylistChange> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 256 || name.chars().any(char::is_control) {
            return Err("Enter a playlist name of at most 256 characters.".into());
        }
        let result: PlaylistChange = self
            .playlist_write(
                "playlists/selection",
                json!({"name":name, "selection":selection(source_ids)?}),
                key,
            )
            .await?;
        check_id(&result.playlist_id)?;
        if result.rule_revision == 0 || result.count != source_ids.len() {
            return Err("Invalid playlist creation response.".into());
        }
        Ok(result)
    }

    pub async fn append_playlist(
        &self,
        id: &str,
        expected_revision: u64,
        source_ids: &[String],
        key: &str,
    ) -> Result<PlaylistChange> {
        let path = format!("playlists/{}/selection", check_id(id)?);
        let body = json!({"expectedRevision":expected_revision,"selection":selection(source_ids)?});
        let result: PlaylistChange = self.playlist_write(&path, body, key).await?;
        if result.playlist_id != id
            || result.rule_revision <= expected_revision
            || result.count != source_ids.len()
        {
            return Err("Invalid playlist append response.".into());
        }
        Ok(result)
    }

    pub async fn rebuild_playlist(
        &self,
        id: &str,
        expected_rule_revision: u64,
        key: &str,
    ) -> Result<u64> {
        #[derive(Deserialize)]
        struct Snapshot {
            revision: u64,
        }
        let path = format!("playlists/{}/rebuild", check_id(id)?);
        let result: Snapshot = self
            .playlist_write(
                &path,
                json!({"expectedRuleRevision":expected_rule_revision}),
                key,
            )
            .await?;
        if result.revision == 0 {
            return Err("Invalid published playlist revision.".into());
        }
        Ok(result.revision)
    }
}
