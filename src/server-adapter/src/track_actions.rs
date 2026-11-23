use super::{check_id, Result, ServerSession};
use serde_json::{json, Value};

fn binding(track_id: &str, pin: Option<&str>) -> Result<Value> {
    check_id(track_id)?;
    if let Some(id) = pin {
        check_id(id)?;
    }
    Ok(
        json!({"kind":"local","entries":[{"source":{"trackId":track_id,"forcedSourceTrackId":pin,"sourceTrackId":null,"remoteSourceId":null}}]}),
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
        let entry = binding(track_id, pin)?;
        let (method, path, body) = match target {
            None => (
                "POST",
                "/playlists".into(),
                json!({"name":name,"bindings":[entry]}),
            ),
            Some((id, revision)) => {
                let path = format!("/playlists/{}", check_id(id)?);
                let list = self.settings_json("GET", &path, None, &[]).await?;
                if list["playlistId"] != id || list["ruleRevision"].as_u64() != Some(revision) {
                    return Err("Playlist changed (HTTP 409).".into());
                }
                let mut bindings = list["bindings"]
                    .as_array()
                    .ok_or("Invalid playlist bindings.")?
                    .clone();
                bindings.push(entry);
                (
                    "PUT",
                    path,
                    json!({"name":list["name"],"bindings":bindings,"expectedRevision":revision}),
                )
            }
        };
        self.settings_json(method, &path, Some(&body), &[("Idempotency-Key", key)])
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
        let automatic = binding("song", None).unwrap();
        let pinned = binding("song", Some("live")).unwrap();
        assert!(automatic["entries"][0]["source"]["forcedSourceTrackId"].is_null());
        assert_eq!(pinned["entries"][0]["source"]["trackId"], "song");
        assert_eq!(
            pinned["entries"][0]["source"]["forcedSourceTrackId"],
            "live"
        );
        assert!(pinned["entries"][0]["source"]["sourceTrackId"].is_null());
    }
}
