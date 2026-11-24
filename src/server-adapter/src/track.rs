use super::check_id;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceVersion {
    pub source_track_id: String,
    pub provider_instance_id: String,
    pub display_name: String,
    pub availability: String,
    pub codec: Option<String>,
    pub container: Option<String>,
    pub bitrate_bps: Option<u64>,
    pub sample_rate_hz: Option<u64>,
    pub duration_ms: Option<u64>,
    pub cover_object_id: Option<String>,
    pub online_cover_path: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceGroup {
    #[serde(flatten)]
    pub default: SourceVersion,
    pub recording_id: Option<String>,
    pub manual_source_track_id: Option<String>,
    #[serde(default)]
    pub versions: Vec<SourceVersion>,
}

impl SourceGroup {
    pub fn choices(&self) -> impl Iterator<Item = &SourceVersion> {
        std::iter::once(&self.default).chain(
            self.versions
                .iter()
                .filter(|s| s.source_track_id != self.default.source_track_id),
        )
    }
}

// 曲目 ID 指向目录资料，固定的来源 ID 只影响本次播放、收藏或歌单引用。
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    #[serde(default)]
    pub artist_credits: Vec<TrackCredit>,
    #[serde(default)]
    pub album_credits: Vec<TrackCredit>,
    pub cover_object_id: Option<String>,
    pub sources: Option<SourceGroup>,
    pub forced_source_track_id: Option<String>,
    pub unresolved: bool,
}

impl Track {
    pub fn source(&self) -> Option<&SourceVersion> {
        let group = self.sources.as_ref()?;
        match &self.forced_source_track_id {
            Some(id) => group.choices().find(|s| &s.source_track_id == id),
            None => Some(&group.default),
        }
    }
    pub fn playable_source_id(&self) -> Option<&str> {
        self.source()
            .filter(|s| s.availability == "available")
            .map(|s| s.source_track_id.as_str())
    }
    pub fn cover(&self) -> Option<&str> {
        self.cover_object_id
            .as_deref()
            .or_else(|| self.source()?.cover_object_id.as_deref())
    }

    pub fn online_cover(&self) -> Option<&str> {
        self.source()
            .and_then(|s| s.online_cover_path.as_deref())
            .or_else(|| {
                self.sources
                    .as_ref()?
                    .choices()
                    .find_map(|s| s.online_cover_path.as_deref())
            })
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TrackCredit {
    #[serde(default, alias = "artistId", alias = "albumId")]
    pub id: String,
    pub name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MusicTrack {
    pub track_id: String,
    title: String,
    artists: Vec<TrackCredit>,
    albums: Vec<TrackCredit>,
    cover_path: Option<String>,
    selected_source: Option<SourceGroup>,
}
impl MusicTrack {
    pub(super) fn into_track(self, forced: Option<String>) -> Track {
        let album = self
            .albums
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ");
        let cover_object_id = self
            .cover_path
            .as_deref()
            .and_then(|p| p.strip_prefix("/api/v1/media/"))
            .and_then(|id| check_id(id).ok())
            .map(str::to_owned);
        Track {
            id: self.track_id,
            title: self.title,
            artists: self.artists.iter().map(|a| a.name.clone()).collect(),
            artist_credits: self.artists,
            album_credits: self.albums,
            album: (!album.is_empty()).then_some(album),
            cover_object_id,
            sources: self.selected_source,
            forced_source_track_id: forced,
            unresolved: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn online_artwork_falls_back_to_another_version_and_uses_the_session_server() {
        let mut selected = json!({"sourceTrackId":"local","displayName":"Song","providerInstanceId":"files","availability":"available"});
        selected["versions"] = json!([{"sourceTrackId":"cloud","displayName":"Song","providerInstanceId":"netease","availability":"available","onlineCoverPath":"/api/v1/source-tracks/cloud/online-artwork"}]);
        let raw = json!({"trackId":"song","title":"Song","artists":[], "albums":[], "selectedSource":selected});
        let track = serde_json::from_value::<MusicTrack>(raw)
            .unwrap()
            .into_track(None);
        let path = track.online_cover().unwrap();
        assert_eq!(
            super::super::cover_endpoint(path).unwrap(),
            "source-tracks/cloud/online-artwork"
        );
        assert_eq!(
            super::super::cover_endpoint("saved").unwrap(),
            "media/saved"
        );
        for invalid in [
            "https://outside.invalid/image",
            "/api/v1/source-tracks/../online-artwork",
            "/api/v1/source-tracks/cloud/online-artwork?source=other",
        ] {
            assert!(super::super::cover_endpoint(invalid).is_err());
        }
    }
    #[test]
    fn track_identity_and_cover_survive_a_missing_or_disabled_pinned_source() {
        let version = |id: &str, availability: &str| json!({"sourceTrackId":id,"displayName":id,"providerInstanceId":"local","availability":availability,"coverObjectId":id});
        let mut selected = version("master", "available");
        selected["versions"] = json!([
            version("master", "available"),
            version("alternate", "available"),
            version("gone", "missing")
        ]);
        let raw = json!({"trackId":"song","title":"Title","albums":[{"albumId":"record","name":"Record"}],
            "artists":[{"artistId":"credit","name":"Artist"}],"coverPath":"/api/v1/media/art","selectedSource":selected});
        for (pin, playable) in [
            (None, Some("master")),
            (Some("alternate"), Some("alternate")),
            (Some("gone"), None),
            (Some("removed"), None),
        ] {
            let t = serde_json::from_value::<MusicTrack>(raw.clone())
                .unwrap()
                .into_track(pin.map(str::to_owned));
            assert_eq!(t.id, "song");
            assert_eq!(t.artist_credits[0].id, "credit");
            assert_eq!(t.album_credits[0].id, "record");
            assert_eq!(t.cover(), Some("art"));
            assert_eq!(t.playable_source_id(), playable);
            assert_eq!(t.sources.unwrap().choices().count(), 3);
        }
    }
}
