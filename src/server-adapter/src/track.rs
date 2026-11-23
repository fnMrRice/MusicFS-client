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
}

#[derive(Deserialize)]
pub(super) struct Name {
    name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MusicTrack {
    pub track_id: String,
    title: String,
    artists: Vec<Name>,
    albums: Vec<Name>,
    cover_path: Option<String>,
    selected_source: Option<SourceGroup>,
}
impl MusicTrack {
    pub(super) fn into_track(self, forced: Option<String>) -> Track {
        let album = self
            .albums
            .into_iter()
            .map(|a| a.name)
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
            artists: self.artists.into_iter().map(|a| a.name).collect(),
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
    fn track_identity_and_cover_survive_a_missing_or_disabled_pinned_source() {
        let version = |id: &str, availability: &str| json!({"sourceTrackId":id,"displayName":id,"providerInstanceId":"local","availability":availability,"coverObjectId":id});
        let mut selected = version("master", "available");
        selected["versions"] = json!([
            version("master", "available"),
            version("alternate", "available"),
            version("gone", "missing")
        ]);
        let raw = json!({"trackId":"song","title":"Title","albums":[],"artists":[],"coverPath":"/api/v1/media/art","selectedSource":selected});
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
            assert_eq!(t.cover(), Some("art"));
            assert_eq!(t.playable_source_id(), playable);
            assert_eq!(t.sources.unwrap().choices().count(), 3);
        }
    }
}
