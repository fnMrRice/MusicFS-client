use super::{check_id, Result, ServerSession};
use serde_json::{json, Value};

fn reference(track_id: &str, pin: Option<&str>) -> Result<Value> {
    check_id(track_id)?;
    if let Some(id) = pin {
        check_id(id)?;
    }
    Ok(
        json!({"source":{"trackId":track_id,"forcedSourceTrackId":pin,"sourceTrackId":null,"remoteSourceId":null}}),
    )
}
impl ServerSession {
    pub async fn add_track_reference(
        &self,
        target: Option<(&str, u64)>,
        name: &str,
        track_id: &str,
        pin: Option<&str>,
        key: &str,
    ) -> Result<()> {
        let refs = [(track_id.to_owned(), pin.map(str::to_owned))];
        self.add_track_references(target, name, &refs, key).await
    }
    pub async fn add_track_references(
        &self,
        target: Option<(&str, u64)>,
        name: &str,
        tracks: &[(String, Option<String>)],
        key: &str,
    ) -> Result<()> {
        let body = self.prepare_track_references(target, name, tracks).await?;
        self.submit_track_references(target.map(|(id, _)| id), &body, key)
            .await
    }
    pub async fn prepare_track_references(
        &self,
        target: Option<(&str, u64)>,
        name: &str,
        tracks: &[(String, Option<String>)],
    ) -> Result<Value> {
        if tracks.is_empty() {
            return Err("The track selection is empty.".into());
        }
        let entries = tracks
            .iter()
            .map(|(id, pin)| reference(id, pin.as_deref()))
            .collect::<Result<Vec<_>>>()?;
        let binding = json!({"kind":"local","entries":entries});
        let Some((id, revision)) = target else {
            return Ok(json!({"name":name,"bindings":[binding]}));
        };
        let path = format!("/playlists/{}", check_id(id)?);
        let list = self.settings_json("GET", &path, None, &[]).await?;
        if list["playlistId"] != id || list["ruleRevision"].as_u64() != Some(revision) {
            return Err("Playlist changed (HTTP 409).".into());
        }
        let mut bindings = list["bindings"]
            .as_array()
            .ok_or("Invalid playlist bindings.")?
            .clone();
        bindings.push(binding);
        Ok(json!({"name":list["name"],"bindings":bindings,"expectedRevision":revision}))
    }
    // 重试提交时复用完整原始请求体；歌单的当前修订可能已被这次请求推进。
    pub async fn submit_track_references(
        &self,
        target: Option<&str>,
        body: &Value,
        key: &str,
    ) -> Result<()> {
        let (method, path) = match target {
            Some(id) => ("PUT", format!("/playlists/{}", check_id(id)?)),
            None => ("POST", "/playlists".to_owned()),
        };
        self.settings_json(method, &path, Some(body), &[("Idempotency-Key", key)])
            .await?;
        Ok(())
    }
    pub async fn favorite_version(&self, track_id: &str, pin: Option<&str>) -> Result<()> {
        if let Some(id) = pin {
            check_id(id)?;
        }
        self.settings_json(
            "PUT",
            &format!("/me/favorites/{}", check_id(track_id)?),
            Some(&json!({"forcedSourceTrackId":pin})),
            &[],
        )
        .await?;
        Ok(())
    }
    pub async fn matching_revision(&self) -> Result<u64> {
        self.settings_json("GET", "/matching/revision", None, &[])
            .await?["revision"]
            .as_u64()
            .ok_or("Invalid matching revision.".into())
    }
    pub async fn choose_default_source(
        &self,
        recording: &str,
        source: Option<&str>,
        revision: u64,
        key: &str,
    ) -> Result<()> {
        let body = json!({"recordingId":check_id(recording)?,"sourceTrackId":source.map(check_id).transpose()?,"expectedRevision":revision});
        self.settings_json(
            "POST",
            "/matching/default-source",
            Some(&body),
            &[("Idempotency-Key", key)],
        )
        .await?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn song_references_only_pin_an_explicit_version() {
        let automatic = reference("song", None).unwrap();
        let pinned = reference("song", Some("live")).unwrap();
        assert!(automatic["source"]["forcedSourceTrackId"].is_null());
        assert_eq!(pinned["source"]["trackId"], "song");
        assert_eq!(pinned["source"]["forcedSourceTrackId"], "live");
        assert!(pinned["source"]["sourceTrackId"].is_null());
    }
}
