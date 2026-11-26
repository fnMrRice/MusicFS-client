use super::Services;
use crate::i18n::tr;
use crate::ui::style;
use eframe::egui;
use musicfs_player_core::Track as QueueTrack;
use musicfs_server_adapter::{SourceVersion, Track};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Evidence {
    pub source_track_id: String,
    pub provider_instance_id: String,
    pub display_name: String,
    pub recording_id: Option<String>,
    title: Option<String>,
    artists: Option<Vec<String>>,
    album: Option<String>,
    codec: Option<String>,
    container: Option<String>,
    duration_ms: Option<u64>,
    availability: String,
    cover_object_id: Option<String>,
    online_cover_path: Option<String>,
}
impl Evidence {
    pub fn version(track: &Track, source: &SourceVersion) -> Self {
        Self {
            source_track_id: source.source_track_id.clone(),
            provider_instance_id: source.provider_instance_id.clone(),
            display_name: source.display_name.clone(),
            recording_id: track.sources.as_ref().and_then(|g| g.recording_id.clone()),
            title: Some(track.title.clone()),
            artists: Some(track.artists.clone()),
            album: track.album.clone(),
            codec: source.codec.clone(),
            container: source.container.clone(),
            duration_ms: source.duration_ms,
            availability: source.availability.clone(),
            cover_object_id: source.cover_object_id.clone(),
            online_cover_path: source.online_cover_path.clone(),
        }
    }
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        error: &mut Option<String>,
        lookup: &dyn Fn(&str) -> String,
    ) {
        let title = self
            .title
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.display_name);
        ui.group(|ui| {
            ui.strong(title);
            let artists = self
                .artists
                .as_ref()
                .filter(|a| !a.is_empty())
                .map(|a| a.join(" / "))
                .unwrap_or_else(|| tr("未知歌手").into());
            ui.weak(artists);
            let mut details = vec![lookup(&self.provider_instance_id)];
            if let Some(format) = self.container.as_ref().or(self.codec.as_ref()) {
                details.push(format.to_uppercase());
            }
            if let Some(ms) = self.duration_ms {
                details.push(format!("{}:{:02}", ms / 60000, ms / 1000 % 60));
            }
            ui.small(details.join(" · "));
            if self.display_name != title {
                ui.small(&self.display_name);
            }
            if style::add_button(
                ui,
                self.availability == "available",
                egui::Button::new(tr("试听")),
            )
            .clicked()
            {
                let track = QueueTrack {
                    id: self.source_track_id.clone(),
                    source_only: true,
                    title: title.into(),
                    artists: self.artists.clone().unwrap_or_default(),
                    album: self.album.clone(),
                    source_track_id: Some(self.source_track_id.clone()),
                    forced_source_track_id: None,
                    duration_ms: self.duration_ms,
                    cover_object_id: self.cover_object_id.clone(),
                    online_cover_path: self.online_cover_path.clone(),
                };
                *error = svc.player.set_queue(vec![track], 0).err();
            }
        });
    }
}

pub(super) fn selected(tracks: &[Track]) -> Vec<Evidence> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for track in tracks {
        for source in track.sources.iter().flat_map(|group| group.choices()) {
            if seen.insert(&source.source_track_id) {
                items.push(Evidence::version(track, source));
            }
        }
    }
    items
}
