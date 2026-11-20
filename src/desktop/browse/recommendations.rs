use super::resource::Resource;
use eframe::egui;
use musicfs_server_adapter::{RecommendationFeed, RecommendedPlaylistDetail, ServerSession};
use std::collections::HashSet;
use tokio::runtime::Handle;

#[derive(Default)]
pub(super) struct Recommendations {
    feed: Resource<RecommendationFeed>,
    requested: (u8, bool),
    offset: u8,
    selected: Option<PlaylistView>,
}

impl Recommendations {
    pub fn show(&mut self, ui: &mut egui::Ui, rt: &Handle, session: &ServerSession) {
        if let Some(selected) = &mut self.selected {
            if ui.button("‹  返回推荐").clicked() {
                self.selected = None;
            } else {
                ui.add_space(12.0);
                selected.show(ui, rt, session);
                return;
            }
        }
        if self.feed.poll() {
            self.offset = self.requested.0;
        }
        let mut request = None;
        let depth = self.feed.value.as_ref().map_or(0, |f| f.history_depth);
        ui.horizontal_wrapped(|ui| {
            if self.offset > 0 {
                ui.weak(format!("历史推荐  {} / {depth}", self.offset));
                if ui
                    .add_enabled(!self.feed.busy(), egui::Button::new("返回最新推荐"))
                    .clicked()
                {
                    request = Some((0, false));
                }
            }
            if depth > 0
                && ui
                    .add_enabled(
                        !self.feed.busy() && self.offset < depth,
                        egui::Button::new("上一期"),
                    )
                    .clicked()
            {
                request = Some((self.offset + 1, false));
            }
            if ui
                .add_enabled(!self.feed.busy(), egui::Button::new("刷新推荐"))
                .clicked()
            {
                request = Some((0, true));
            }
        });
        let retry = self.feed.status(ui, "正在读取推荐…");
        if !self.feed.started || retry {
            request = Some(self.requested);
        }
        if let Some((history, refresh)) = request {
            let server = session.clone();
            self.requested = (history, refresh);
            self.feed.start(ui.ctx(), rt, async move {
                server.recommendations(history, refresh).await
            });
        }
        let Some(feed) = &self.feed.value else {
            return;
        };
        ui.add_space(12.0);
        if feed.sources.is_empty() {
            ui.heading("暂无推荐来源");
            ui.weak("连接的音乐库尚未提供推荐歌单。");
        }
        for source in &feed.sources {
            ui.push_id(&source.instance_id, |ui| {
                ui.heading(source.display_name.as_deref().unwrap_or("未知来源"));
                if source.stale {
                    let date = source
                        .last_success_at
                        .as_deref()
                        .and_then(|s| s.get(..10))
                        .unwrap_or("");
                    ui.weak(format!("显示上次成功获取的推荐  {date}"));
                }
                if let Some(error) = &source.error {
                    let message = if error.code == "PLATFORM_AUTH_REQUIRED" {
                        "此来源需要登录平台账号。".to_owned()
                    } else {
                        format!("此来源暂时不可用（{}）。", error.code)
                    };
                    ui.colored_label(ui.visuals().warn_fg_color, message);
                } else if source.items.is_empty() {
                    ui.weak("此来源暂无推荐歌单。");
                }
                let columns = (ui.available_width() / 200.0).floor().clamp(1.0, 4.0) as usize;
                for row in source.items.chunks(columns) {
                    ui.columns(columns, |widgets| {
                        for (ui, item) in widgets.iter_mut().zip(row) {
                            let playlist = &item.playlist;
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                                if ui
                                    .link(egui::RichText::new(&playlist.title).size(16.0).strong())
                                    .clicked()
                                {
                                    self.selected = Some(PlaylistView::new(
                                        source.instance_id.clone(),
                                        playlist.external_id.clone(),
                                    ));
                                }
                                if let Some(creator) = &playlist.creator {
                                    ui.label(creator);
                                }
                                if let Some(count) = playlist.track_count {
                                    ui.weak(format!("{count} 首"));
                                }
                                if !item.reason.is_empty() {
                                    ui.small(&item.reason);
                                }
                            });
                        }
                    });
                    ui.add_space(6.0);
                }
                ui.add_space(12.0);
            });
        }
    }
}

struct PlaylistView {
    instance: String,
    external: String,
    request: Resource<RecommendedPlaylistDetail>,
    details: Option<RecommendedPlaylistDetail>,
    seen: HashSet<String>,
    visible: usize,
}

impl PlaylistView {
    fn new(instance: String, external: String) -> Self {
        Self {
            instance,
            external,
            request: Resource::default(),
            details: None,
            seen: HashSet::new(),
            visible: 50,
        }
    }

    fn load(&mut self, ctx: &egui::Context, rt: &Handle, session: &ServerSession) {
        let (server, instance, external) = (
            session.clone(),
            self.instance.clone(),
            self.external.clone(),
        );
        let next = self.details.as_ref().and_then(|p| p.next_cursor.clone());
        self.request.start(ctx, rt, async move {
            server
                .recommendation_playlist(&instance, &external, next.as_deref())
                .await
        });
    }

    fn show(&mut self, ui: &mut egui::Ui, rt: &Handle, session: &ServerSession) {
        if self.request.poll() {
            if let Some(mut page) = self.request.value.take() {
                if page
                    .next_cursor
                    .as_ref()
                    .is_some_and(|cursor| !self.seen.insert(cursor.clone()))
                {
                    self.request.error = Some("歌单分页已发生变化，请返回后重新打开。".into());
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
        let retry = self.request.status(ui, "正在读取在线歌单…");
        if !self.request.started || retry {
            self.load(ui.ctx(), rt, session);
        }
        let Some(detail) = &self.details else {
            return;
        };
        ui.heading(&detail.playlist.title);
        ui.horizontal_wrapped(|ui| {
            if let Some(creator) = &detail.playlist.creator {
                ui.label(creator);
            }
            ui.weak(detail.playlist.track_count.map_or_else(
                || format!("已读取 {} 首", detail.entries.len()),
                |n| format!("{n} 首"),
            ));
            ui.weak(detail.observed_at.get(..10).unwrap_or(&detail.observed_at));
        });
        if !detail.complete {
            ui.weak("来源只返回了部分曲目。");
        }
        ui.add_space(10.0);
        ui.separator();
        if detail.entries.is_empty() {
            ui.weak("此歌单暂无可见曲目。");
        }
        for entry in detail.entries.iter().take(self.visible) {
            ui.horizontal_wrapped(|ui| {
                ui.weak(format!("{:02}", entry.position + 1));
                if let Some(track) = &entry.track {
                    ui.label(&track.title);
                    if !track.artists.is_empty() {
                        ui.weak(track.artists.join(" / "));
                    }
                    if let Some(ms) = track.duration_ms {
                        ui.small(format!("{}:{:02}", ms / 60_000, ms / 1000 % 60));
                    }
                } else if entry.unavailable {
                    ui.weak("曲目已失效");
                } else {
                    ui.label(&entry.external_track_id);
                }
            });
            ui.separator();
        }
        if detail.entries.len() > self.visible {
            if ui.button("显示更多曲目").clicked() {
                self.visible = self.visible.saturating_add(50);
            }
        } else if detail.next_cursor.is_some()
            && !self.request.busy()
            && self.request.error.is_none()
            && ui.button("加载更多曲目").clicked()
        {
            self.load(ui.ctx(), rt, session);
        }
    }
}
