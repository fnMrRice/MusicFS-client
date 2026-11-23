#[path = "artwork.rs"]
mod artwork;
use super::data::{queue_track, CategoryTab, Pages, Services, TrackQuery};
use super::style;
use crate::task::Task;
use eframe::egui::{self, RichText};
use musicfs_player_core::Track as QueueTrack;
use musicfs_server_adapter::{Category, Track};
use std::collections::HashSet;

#[derive(Default)]
pub struct Catalog {
    pub artwork: artwork::Artwork,
    pub tabs: CategoryTab,
    pub category: Option<(String, String)>,
    pub categories: Pages<Category>,
    pub tracks: Pages<Track>,
    query: Option<TrackQuery>,
    complete: Option<Task<Vec<QueueTrack>>>,
    pub search_draft: String,
    pub search: String,
    pub focus_search: bool,
}

impl Catalog {
    pub fn reset_tracks(&mut self) {
        self.tracks = Pages::default();
        self.query = None;
        self.complete = None;
    }
    pub fn reset_categories(&mut self) {
        self.category = None;
        self.categories = Pages::default();
        self.reset_tracks();
    }
    pub fn search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.search_draft)
                    .hint_text("搜索歌曲、歌手或专辑")
                    .desired_width((ui.available_width() - 95.0).clamp(120.0, 460.0)),
            );
            if self.focus_search {
                field.request_focus();
                self.focus_search = false;
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if style::primary(ui, "搜索", !self.search_draft.trim().is_empty()).clicked() || enter
            {
                self.search = self.search_draft.trim().into();
                self.reset_tracks();
            }
        });
        ui.add_space(10.0);
    }

    pub fn library(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> Option<TrackQuery> {
        ui.horizontal_wrapped(|ui| {
            for tab in [
                CategoryTab::All,
                CategoryTab::Artist,
                CategoryTab::Album,
                CategoryTab::Genre,
            ] {
                if ui
                    .selectable_label(self.tabs == tab, RichText::new(tab.label()).size(16.0))
                    .clicked()
                {
                    self.tabs = tab;
                    self.reset_categories();
                }
            }
        });
        ui.separator();
        let Some(kind) = self.tabs.kind() else {
            return Some(TrackQuery::Library(String::new()));
        };
        if let Some((id, name)) = self.category.clone() {
            let mut back = false;
            ui.horizontal(|ui| {
                back = ui.link("返回分类").clicked();
                ui.weak("/");
                ui.strong(if name.is_empty() {
                    format!("未知{}", self.tabs.label())
                } else {
                    name
                });
            });
            if back {
                self.category = None;
                self.reset_tracks();
            } else {
                return Some(TrackQuery::Category { tab: self.tabs, id });
            }
        }
        self.categories.poll();
        let mut load = !self.categories.started;
        ui.horizontal(|ui| {
            if ui.button("刷新分类").clicked() {
                self.categories = Pages::default();
                load = true;
            }
            if self.categories.busy() {
                ui.spinner();
            }
        });
        ui.add_space(6.0);
        if let Some(error) = self.categories.error.as_ref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button("重试加载分类").clicked() {
                load = true;
            }
        } else if self.categories.started
            && !self.categories.busy()
            && self.categories.items.is_empty()
        {
            ui.weak(format!("音乐库中暂无{}。", self.tabs.label()));
        }
        egui::ScrollArea::vertical()
            .id_salt("category-list")
            .show(ui, |ui| {
                let count = (ui.available_width() / 200.0).max(1.0) as usize;
                for row in self.categories.items.chunks(count) {
                    ui.horizontal(|ui| {
                        let width = ((ui.available_width()
                            - (count - 1) as f32 * ui.spacing().item_spacing.x)
                            / count as f32)
                            .min(260.0);
                        for entry in row {
                            let name = if entry.name.is_empty() {
                                format!("未知{}", self.tabs.label())
                            } else {
                                entry.name.clone()
                            };
                            if ui
                                .add_sized(
                                    [width, 55.0],
                                    egui::Button::new(format!("{name}\n{} 首", entry.track_count))
                                        .truncate(),
                                )
                                .clicked()
                            {
                                self.category = Some((entry.id.clone(), entry.name.clone()));
                            }
                        }
                    });
                }
                if self.categories.next.is_some()
                    && ui
                        .add_enabled(!self.categories.busy(), egui::Button::new("加载更多分类"))
                        .clicked()
                {
                    load = true;
                }
            });
        if load {
            let server = svc.server.clone();
            let cursor = self.categories.next.clone();
            self.categories.load(ui.ctx(), svc.runtime, async move {
                server.categories(kind, cursor.as_deref()).await
            });
        }
        None
    }

    pub fn sync(&mut self, ui: &egui::Ui, svc: Services<'_>, query: &TrackQuery) {
        if self.query.as_ref() != Some(query) {
            self.reset_tracks();
            self.query = Some(query.clone());
        }
        self.tracks.poll();
        if !self.tracks.started {
            self.more(ui, svc);
        }
    }
    pub fn more(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let Some(query) = self.query.clone() else {
            return;
        };
        let cursor = self.tracks.next.clone();
        let server = svc.server.clone();
        self.tracks.load(ui.ctx(), svc.runtime, async move {
            query.fetch(&server, cursor.as_deref()).await
        });
    }

    pub fn play_all(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> Option<String> {
        if let Some(result) = self.complete.as_mut().and_then(Task::poll) {
            self.complete = None;
            return match result {
                Ok(queue) if queue.is_empty() => Some("列表中没有可播放的音源。".into()),
                Ok(queue) => svc.player.set_queue(queue, 0).err(),
                Err(error) => Some(error),
            };
        }
        let available = self.complete.is_none()
            && !self.tracks.busy()
            && (self.tracks.next.is_some()
                || self
                    .tracks
                    .items
                    .iter()
                    .any(|t| t.playable_source_id().is_some()));
        if style::primary(
            ui,
            if self.complete.is_some() {
                "正在读取…"
            } else {
                "播放全部"
            },
            available,
        )
        .clicked()
        {
            let Some(query) = self.query.clone() else {
                return None;
            };
            let mut cursor = self.tracks.next.clone();
            let mut queue: Vec<_> = self.tracks.items.iter().filter_map(queue_track).collect();
            if cursor.is_none() {
                return svc.player.set_queue(queue, 0).err();
            }
            let server = svc.server.clone();
            self.complete = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                let mut visited = HashSet::new();
                while let Some(after) = cursor {
                    if !visited.insert(after.clone()) {
                        return Err("服务器分页未向前推进。".into());
                    }
                    let page = query.fetch(&server, Some(&after)).await?;
                    queue.extend(page.items.iter().filter_map(queue_track));
                    cursor = page.next_cursor;
                }
                Ok(queue)
            }));
        }
        None
    }
}
