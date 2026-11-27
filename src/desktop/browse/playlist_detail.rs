use super::{resource::Resource, QueueSelection};
use crate::{
    i18n::tr,
    ui::{
        online_tracks,
        playlist_tracks::{Metadata, PlaylistTools},
        Artwork, Services,
    },
};
use eframe::egui;
use musicfs_player_core::Track;
use musicfs_server_adapter::{RecommendedPlaylistDetail, TrackInfo};
use std::collections::HashSet;

pub(crate) struct Detail {
    instance: String,
    source_name: String,
    external: String,
    cover_url: Option<String>,
    can_play: bool,
    request: Resource<RecommendedPlaylistDetail>,
    playback: Resource<Track>,
    details: Option<RecommendedPlaylistDetail>,
    seen: HashSet<String>,
    tools: PlaylistTools,
    order: Vec<usize>,
}

impl Detail {
    pub fn new(
        instance: &str,
        external: &str,
        cover_url: Option<String>,
        can_play: bool,
        source_name: String,
    ) -> Self {
        Self {
            instance: instance.into(),
            source_name: if source_name.trim().is_empty() || source_name == instance {
                tr("未知来源").to_owned()
            } else {
                source_name
            },
            external: external.into(),
            cover_url,
            can_play,
            request: Resource::default(),
            playback: Resource::default(),
            details: None,
            seen: HashSet::new(),
            tools: Default::default(),
            order: Vec::new(),
        }
    }

    pub fn playlist(&self) -> Option<&musicfs_server_adapter::online_playlists::OnlinePlaylist> {
        self.details.as_ref().map(|d| &d.playlist)
    }

    pub fn key(&self) -> (&str, &str) {
        (&self.instance, &self.external)
    }

    fn load(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let (server, instance, external) = (
            svc.server.clone(),
            self.instance.clone(),
            self.external.clone(),
        );
        let next = self.details.as_ref().and_then(|p| p.next_cursor.clone());
        self.request.start(ctx, svc.runtime, async move {
            server
                .recommendation_playlist(&instance, &external, next.as_deref())
                .await
        });
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        artwork: &mut Artwork,
    ) -> Option<QueueSelection> {
        self.playback.poll();
        let result = self.playback.value.take().map(|track| (vec![track], 0));
        let mut first_new = None;
        if self.request.poll() {
            if let Some(mut page) = self.request.value.take() {
                if page
                    .next_cursor
                    .as_ref()
                    .is_some_and(|cursor| !self.seen.insert(cursor.clone()))
                {
                    self.request.error = Some(tr("歌单分页已发生变化，请返回后重新打开。").into());
                } else {
                    first_new = Some(self.details.as_ref().map_or(0, |old| old.entries.len()));
                    if let Some(old) = &mut self.details {
                        old.entries.append(&mut page.entries);
                        std::mem::swap(&mut old.entries, &mut page.entries);
                    }
                    self.details = Some(page);
                }
            }
        }
        let Some(detail) = &self.details else {
            let retry = self.request.status(ui, tr("正在读取在线歌单…"));
            if !self.request.started || retry {
                self.load(ui.ctx(), svc);
            }
            return result;
        };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(76.), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                artwork.paint(
                    ui,
                    rect,
                    svc,
                    detail
                        .playlist
                        .cover_url
                        .as_deref()
                        .or(self.cover_url.as_deref()),
                );
            }
            ui.vertical(|ui| {
                ui.heading(&detail.playlist.title);
                if let Some(creator) = &detail.playlist.creator {
                    ui.label(creator);
                }
                ui.horizontal_wrapped(|ui| {
                    ui.weak(detail.playlist.track_count.map_or_else(
                        || tr("已读取 {0} 首").replace("{0}", &detail.entries.len().to_string()),
                        |n| tr("{0} 首").replace("{0}", &n.to_string()),
                    ));
                    ui.weak(detail.observed_at.get(..10).unwrap_or(&detail.observed_at));
                });
            });
        });
        if !detail.complete {
            ui.weak(tr("来源只返回了部分曲目。"));
        }
        ui.add_space(10.0);
        let changed = ui.horizontal(|ui| self.tools.show(ui)).inner;
        if changed {
            self.order.clear();
            first_new = Some(0);
        }
        if let Some(start) = first_new {
            let metadata = |i: usize| {
                let entry = &detail.entries[i];
                match &entry.track {
                    Some(track) => Metadata {
                        title: &track.title,
                        artists: &track.artists,
                        album: track.album.as_deref(),
                    },
                    None => Metadata {
                        title: &entry.external_track_id,
                        artists: &[],
                        album: None,
                    },
                }
            };
            self.order.extend(
                (start..detail.entries.len()).filter(|&i| self.tools.matches(&metadata(i))),
            );
            if self.tools.sorted() {
                self.order
                    .sort_by_cached_key(|&i| self.tools.sort_key(metadata(i)));
            }
        }
        let has_more = detail.next_cursor.is_some();
        let retry = has_more && self.request.status(ui, tr("正在读取在线歌单…"));
        if self.playback.busy() {
            ui.spinner();
        }
        if let Some(error) = &self.playback.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.order.is_empty() && !has_more {
            ui.vertical_centered(|ui| {
                ui.add_space(70.);
                ui.weak(if self.tools.searching() {
                    tr("没有找到相关曲目")
                } else {
                    tr("此歌单暂无可见曲目。")
                });
            });
            return result;
        }
        let mut scroll =
            egui::ScrollArea::vertical().id_salt(("online-playlist-tracks", self.key()));
        if changed {
            scroll = scroll.vertical_scroll_offset(0.);
        }
        let (selected, at_end) = online_tracks(
            ui,
            scroll,
            detail,
            &self.order,
            &self.source_name,
            self.can_play && !self.playback.busy(),
            has_more && self.request.error.is_none(),
            |ui, rect, url| artwork.paint(ui, rect, svc, url),
        );
        let play = selected.and_then(|i| detail.entries[i].track.clone());
        if has_more
            && (retry
                || (!self.request.busy()
                    && self.request.error.is_none()
                    && (self.tools.active() || at_end)))
        {
            self.load(ui.ctx(), svc);
        }
        if let Some(track) = play {
            self.play_track(ui, svc, track);
        }
        result
    }

    fn play_track(&mut self, ui: &egui::Ui, svc: Services<'_>, track: TrackInfo) {
        let server = svc.server.clone();
        let instance = self.instance.clone();
        self.playback.start(ui.ctx(), svc.runtime, async move {
            crate::ui::resolve_online_queue_track(&server, &instance, track).await
        });
    }
}
