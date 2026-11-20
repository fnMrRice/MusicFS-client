use super::data::Services;
use super::favorites::Favorites;
use super::style;
use crate::task::Task;
use eframe::egui;
use musicfs_server_adapter::{Playlist, Track};
use uuid::Uuid;

#[derive(Clone, PartialEq)]
enum Target {
    Existing(String, u64),
    New(String),
}
enum Change {
    Favorite(String, bool),
    Playlist,
}
struct Editor {
    source: String,
    title: String,
    target_id: Option<String>,
    name: String,
    retry: Option<(Target, String)>,
}

#[derive(Default)]
pub struct Actions {
    pending: Option<Task<Change>>,
    editor: Option<Editor>,
    rebuild_key: Option<(String, u64, String)>,
    pub favorites: Favorites,
    pub notice: Option<(String, bool)>,
}

#[derive(Default)]
pub struct Update {
    pub playlists: bool,
    pub favorites: bool,
}

impl Actions {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn open(&mut self, track: &Track, lists: &[Playlist]) {
        if let Some(source) = &track.source_track_id {
            self.editor = Some(Editor {
                source: source.clone(),
                title: track.title.clone(),
                target_id: lists.first().map(|p| p.id.clone()),
                name: String::new(),
                retry: None,
            });
            self.notice = None;
        }
    }
    pub fn toggle(&mut self, ui: &egui::Ui, svc: Services<'_>, track: &Track, value: bool) {
        if self.busy() {
            return;
        }
        let server = svc.server.clone();
        let id = track.id.clone();
        self.notice = None;
        self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            server.set_favorite(&id, value).await?;
            Ok(Change::Favorite(id, value))
        }));
    }
    pub fn rebuild(&mut self, ui: &egui::Ui, svc: Services<'_>, list: &Playlist) {
        if self.busy() {
            return;
        }
        let revision = list.rule_revision;
        let id = list.id.clone();
        let key = match &self.rebuild_key {
            Some((last, rev, key)) if *last == id && *rev == revision => key.clone(),
            _ => Uuid::new_v4().to_string(),
        };
        self.rebuild_key = Some((id.clone(), revision, key.clone()));
        let server = svc.server.clone();
        self.notice = None;
        self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            server.rebuild_playlist(&id, revision, &key).await?;
            Ok(Change::Playlist)
        }));
    }
    pub fn poll(&mut self) -> Update {
        let mut update = Update::default();
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(Change::Favorite(id, enabled)) => {
                    self.favorites.record(id, enabled);
                    self.notice = Some((
                        (if enabled {
                            "已加入收藏"
                        } else {
                            "已取消收藏"
                        })
                        .into(),
                        false,
                    ));
                    update.favorites = true;
                }
                Ok(Change::Playlist) => {
                    self.notice = Some((
                        (if self.editor.is_some() {
                            "已加入歌单"
                        } else {
                            "歌单内容已更新"
                        })
                        .into(),
                        false,
                    ));
                    self.editor = None;
                    self.rebuild_key = None;
                    update.playlists = true;
                }
                Err(error) => {
                    if error.contains("HTTP 412") || error.contains("HTTP 409") {
                        update.playlists = true;
                    }
                    self.notice = Some((error, true));
                }
            }
        }
        update
    }
    pub fn show_editor(&mut self, ctx: &egui::Context, svc: Services<'_>, lists: &[Playlist]) {
        let busy = self.busy();
        let Some(ed) = &mut self.editor else {
            return;
        };
        let mut close = false;
        let mut apply = None;
        egui::Window::new("加入歌单")
            .id(egui::Id::new("add-to-playlist"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .min_width(310.0)
            .show(ctx, |ui| {
                ui.strong(&ed.title);
                ui.separator();
                ui.add_enabled_ui(!busy, |ui| {
                    let existing = lists.iter().find(|p| Some(&p.id) == ed.target_id.as_ref());
                    egui::ComboBox::from_label("目标歌单")
                        .selected_text(existing.map(|p| p.name.as_str()).unwrap_or("新建歌单…"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut ed.target_id, None, "新建歌单…");
                            for p in lists {
                                ui.selectable_value(&mut ed.target_id, Some(p.id.clone()), &p.name);
                            }
                        });
                    if ed.target_id.is_none() || existing.is_none() {
                        ui.label("新歌单名称");
                        ui.add(
                            egui::TextEdit::singleline(&mut ed.name)
                                .hint_text("输入歌单名称")
                                .desired_width(280.0),
                        );
                    }
                });
                if let Some((error, true)) = &self.notice {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.horizontal(|ui| {
                    let existing = lists.iter().find(|p| Some(&p.id) == ed.target_id.as_ref());
                    let name = ed.name.trim();
                    if style::primary(
                        ui,
                        if busy { "正在保存…" } else { "保存" },
                        !busy && (existing.is_some() || !name.is_empty()),
                    )
                    .clicked()
                    {
                        let target = existing
                            .map(|p| Target::Existing(p.id.clone(), p.rule_revision))
                            .unwrap_or_else(|| Target::New(name.to_owned()));
                        let key = ed
                            .retry
                            .as_ref()
                            .filter(|(t, _)| *t == target)
                            .map(|(_, k)| k.clone())
                            .unwrap_or_else(|| Uuid::new_v4().to_string());
                        ed.retry = Some((target.clone(), key.clone()));
                        apply = Some((target, key, ed.source.clone()));
                    }
                    close = ui.add_enabled(!busy, egui::Button::new("取消")).clicked();
                });
            });
        if close {
            self.editor = None;
        }
        if let Some((target, key, source)) = apply {
            self.notice = None;
            let server = svc.server.clone();
            self.pending = Some(Task::spawn(svc.runtime, ctx, async move {
                let sources = [source];
                match target {
                    Target::Existing(id, revision) => {
                        server
                            .append_playlist(&id, revision, &sources, &key)
                            .await?
                    }
                    Target::New(name) => server.create_playlist(&name, &sources, &key).await?,
                };
                Ok(Change::Playlist)
            }));
        }
    }
}
