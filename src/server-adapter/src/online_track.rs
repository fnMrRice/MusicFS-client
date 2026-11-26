use super::{
    check_id,
    covers::{cover_endpoint, media_object_id},
    decode, Result, ServerSession,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub external_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: Option<u64>,
    pub album: Option<String>,
    pub cover_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedOnlineTrack {
    pub source_track_id: String,
    provider_instance_id: String,
    asset_id: String,
    availability: String,
    pub title: Option<String>,
    pub artists: Option<Vec<String>>,
    pub duration_ms: Option<u64>,
    pub album: Option<String>,
    pub cover_object_id: Option<String>,
    pub online_cover_path: Option<String>,
}

impl ResolvedOnlineTrack {
    fn validate(mut self, instance: &str) -> Result<Self> {
        if self.provider_instance_id != instance
            || self.asset_id.is_empty()
            || self.availability != "available"
        {
            return Err("Invalid online audio resource.".into());
        }
        check_id(&self.source_track_id)?;
        self.cover_object_id = self
            .cover_object_id
            .as_deref()
            .and_then(|cover| media_object_id(cover).ok())
            .map(str::to_owned);
        self.online_cover_path = self
            .online_cover_path
            .filter(|p| p.starts_with("/api/v1/") && cover_endpoint(p).is_ok());
        Ok(self)
    }
}

pub(super) fn online_id(id: &str) -> Result<&str> {
    let bytes = id.as_bytes();
    if bytes.len() <= 128
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || b":_.-".contains(c))
    {
        Ok(id)
    } else {
        Err("Invalid online resource identifier.".into())
    }
}

impl ServerSession {
    pub async fn resolve_online_track(
        &self,
        instance: &str,
        external: &str,
    ) -> Result<ResolvedOnlineTrack> {
        let url = self.url(&format!(
            "online-instances/{}/tracks/{}/resolve",
            check_id(instance)?,
            online_id(external)?
        ));
        let body = serde_json::json!({});
        let key = uuid::Uuid::new_v4().to_string();
        let response = self
            .send_with_headers(Method::POST, url, Some(&body), &[("Idempotency-Key", &key)])
            .await?;
        decode::<ResolvedOnlineTrack>(response)
            .await?
            .validate(instance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolved_covers_accept_media_ids_and_shared_online_artwork_paths() {
        let resolved =
            |cover, online| {
                serde_json::from_value::<ResolvedOnlineTrack>(json!({
            "providerInstanceId": "provider", "assetId": "asset", "availability": "available",
            "sourceTrackId": "playable", "coverObjectId": cover, "onlineCoverPath": online
        })).unwrap().validate("provider").unwrap()
            };
        let shared = "/api/v1/source-tracks/another-version/online-artwork";
        for cover in ["art", "/api/v1/media/art"] {
            let item = resolved(cover, shared);
            assert_eq!(item.cover_object_id.as_deref(), Some("art"));
            assert_eq!(item.online_cover_path.as_deref(), Some(shared));
        }
        let invalid = resolved("/api/v1/media/../art", "/api/v1/private");
        assert!(invalid.cover_object_id.is_none() && invalid.online_cover_path.is_none());
    }
}
