use super::super::{resource::Resource, QueueSelection};
use crate::{
    i18n::tr,
    ui::{Artwork, Services},
};
use eframe::egui;
use musicfs_player_core::Track;
use musicfs_server_adapter::{RecommendedPlaylistDetail, TrackInfo};
use std::collections::HashSet;

pub(super) struct Detail {
    instance: String,
    external: String,
    cover_url: Option<String>,
    can_play: bool,
    request: Resource<RecommendedPlaylistDetail>,
    playback: Resource<Track>,
    details: Option<RecommendedPlaylistDetail>,
    seen: HashSet<String>,
    visible: usize,
}

impl Detail {
    pub fn new(instance: &str, external: &str, cover_url: Option<String>, can_play: bool) -> Self {
        Self {
            instance: instance.into(),
            external: external.into(),
            cover_url,
            can_play,
            request: Resource::default(),
            playback: Resource::default(),
            details: None,
            seen: HashSet::new(),
            visible: 50,
        }
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
        if self.request.poll() {
            if let Some(mut page) = self.request.value.take() {
                if page
                    .next_cursor
                    .as_ref()
                    .is_some_and(|cursor| !self.seen.insert(cursor.clone()))
                {
                    self.request.error = Some(tr("歌单分页已发生变化，请返回后重新打开。").into());
                } else {
                    if let Some(old) = &mut self.details {
                        old.entries.append(&mut page.entries);
                        std::mem::swap(&mut old.entries, &mut page.entries);
                        self.visible = self.visible.saturating_add(50);
                    }
                    self.details = Some(page);
                }
            }
        }
        let retry = self.request.status(ui, tr("正在读取在线歌单…"));
        if !self.request.started || retry {
            self.load(ui.ctx(), svc);
        }
        let Some(detail) = &self.details else {
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
        ui.separator();
        if detail.entries.is_empty() {
            ui.weak(tr("此歌单暂无可见曲目。"));
        }
        let mut play = None;
        if self.playback.busy() {
            ui.spinner();
        }
        if let Some(error) = &self.playback.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        for entry in detail.entries.iter().take(self.visible) {
            ui.horizontal_wrapped(|ui| {
                ui.weak(format!("{:02}", entry.position + 1));
                if let Some(track) = &entry.track {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(36.), egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        artwork.paint(ui, rect, svc, track.cover_url.as_deref());
                    }
                    ui.label(&track.title);
                    if !track.artists.is_empty() {
                        ui.weak(track.artists.join(" / "));
                    }
                    if let Some(ms) = track.duration_ms {
                        ui.small(format!("{}:{:02}", ms / 60_000, ms / 1000 % 60));
                    }
                    if self.can_play
                        && !entry.unavailable
                        && ui
                            .add_enabled(!self.playback.busy(), egui::Button::new(tr("播放")))
                            .clicked()
                    {
                        play = Some(track.clone());
                    }
                } else if entry.unavailable {
                    ui.weak(tr("曲目已失效"));
                } else {
                    ui.label(&entry.external_track_id);
                }
            });
            ui.separator();
        }
        if detail.entries.len() > self.visible {
            if ui.button(tr("显示更多曲目")).clicked() {
                self.visible = self.visible.saturating_add(50);
            }
        } else if detail.next_cursor.is_some()
            && !self.request.busy()
            && self.request.error.is_none()
            && ui.button(tr("加载更多曲目")).clicked()
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
