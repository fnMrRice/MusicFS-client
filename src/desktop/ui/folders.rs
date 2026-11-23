use super::{catalog::providers::Providers, data::Services};
use crate::{i18n::tr, task::Task};
use eframe::egui;
use musicfs_player_core::Track;
use musicfs_server_adapter::folders::{Cursor, Folder, FolderPage, Recording};
use std::{collections::HashSet, future::Future};

struct Pages<T> {
    rows: Vec<T>,
    cursor: Option<Cursor>,
    seen: HashSet<String>,
    total: Option<usize>,
    task: Option<Task<FolderPage<T>>>,
    error: Option<String>,
}
impl<T> Default for Pages<T> {
    fn default() -> Self {
        Self {
            rows: vec![],
            cursor: None,
            seen: HashSet::new(),
            total: None,
            task: None,
            error: None,
        }
    }
}
impl<T: Send + 'static> Pages<T> {
    fn load(
        &mut self,
        ui: &egui::Ui,
        svc: Services<'_>,
        f: impl Future<Output = Result<FolderPage<T>, String>> + Send + 'static,
    ) {
        if self.task.is_none() {
            self.error = None;
            self.task = Some(Task::spawn(svc.runtime, ui.ctx(), f));
        }
    }
    fn poll(&mut self) {
        let Some(r) = self.task.as_mut().and_then(Task::poll) else {
            return;
        };
        self.task = None;
        match r {
            Ok(page)
                if page
                    .next
                    .as_ref()
                    .is_some_and(|c| !self.seen.insert(c.token.clone())) =>
            {
                self.error = Some(tr("服务器分页未向前推进，请刷新列表。").into());
                self.cursor = None;
            }
            Ok(page) => {
                self.rows.extend(page.items);
                self.total = Some(page.total);
                self.cursor = page.next;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn request(&mut self, ui: &mut egui::Ui, more: &str) -> bool {
        if self.task.is_some() {
            ui.spinner();
            false
        } else if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
            if ui.button(tr("重试")).clicked() {
                if self.cursor.is_none()
                    || e.contains("CURSOR")
                    || e.contains("EXPIRED")
                    || e.contains("HTTP 410")
                {
                    *self = Default::default();
                }
                true
            } else {
                false
            }
        } else if self.total.is_none() {
            true
        } else {
            self.cursor.is_some() && ui.button(tr(more)).clicked()
        }
    }
}
#[derive(Default)]
pub(super) struct Folders {
    provider: Option<String>,
    path: Vec<Folder>,
    search: String,
    draft: String,
    children: Pages<Folder>,
    tracks: Pages<Recording>,
    all: Option<Task<Vec<Track>>>,
    message: Option<String>,
}
impl Folders {
    pub(super) fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>, providers: &Providers) {
        if self.provider != providers.selected {
            *self = Self {
                provider: providers.selected.clone(),
                ..Default::default()
            };
        }
        let mut go = None;
        ui.horizontal_wrapped(|ui| {
            if ui.link(tr("文件夹")).clicked() {
                go = Some(0);
            }
            for (i, folder) in self.path.iter().enumerate() {
                ui.weak("/");
                if ui.link(label(folder, providers)).clicked() {
                    go = Some(i + 1);
                }
            }
        });
        if let Some(n) = go {
            if n != self.path.len() {
                self.path.truncate(n);
                self.change_folder();
            }
        }
        ui.horizontal_wrapped(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.draft)
                    .char_limit(256)
                    .hint_text(tr(if self.path.is_empty() {
                        "搜索分类"
                    } else {
                        "搜索当前列表"
                    }))
                    .desired_width(220.),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button(tr("搜索")).clicked() || enter {
                self.search = self.draft.trim().into();
                self.reload();
            }
            if !self.search.is_empty() && ui.button(tr("清空")).clicked() {
                self.draft.clear();
                self.search.clear();
                self.reload();
            }
            if ui.button(tr("刷新列表")).clicked() {
                self.reload();
            }
        });
        self.children.poll();
        self.tracks.poll();
        if let Some(result) = self.all.as_mut().and_then(Task::poll) {
            self.all = None;
            self.message = result
                .and_then(|q| {
                    if q.is_empty() {
                        return Err(tr("列表中没有可播放的音源。").into());
                    }
                    svc.player.set_queue(q, 0).map(|_| ())
                })
                .err();
        }
        if let Some(e) = &self.message {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        let folder = self.path.last().map(|f| f.category_id.clone());
        if folder.is_some() {
            ui.horizontal(|ui| {
                if let Some(total) = self.tracks.total {
                    ui.label(tr("{0} 首").replace("{0}", &total.to_string()));
                }
                if ui
                    .add_enabled(
                        self.all.is_none()
                            && (self.tracks.rows.iter().any(playable)
                                || self.tracks.cursor.is_some()),
                        egui::Button::new(tr("播放全部")),
                    )
                    .clicked()
                {
                    self.play_all(ui, svc);
                }
                if self.all.is_some() {
                    ui.spinner();
                }
            });
        }
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt(("catalog-folders", folder.as_deref(), &self.search))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for f in &self.children.rows {
                    let title = label(f, providers);
                    let count = tr("{0} 首").replace("{0}", &f.track_count.to_string());
                    ui.push_id(&f.category_id, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button(title).clicked() {
                                open = Some(f.clone());
                            }
                            ui.weak(count);
                            if self.provider.is_none() {
                                ui.weak(providers.label(&f.provider_instance_id));
                            }
                        })
                    });
                }
                if self.children.request(ui, "加载更多目录") {
                    let (s, prov, parent, cursor) = (
                        svc.server.clone(),
                        self.provider.clone(),
                        folder.clone(),
                        self.children.cursor.clone(),
                    );
                    let q = if parent.is_none() {
                        self.search.clone()
                    } else {
                        String::new()
                    };
                    self.children.load(ui, svc, async move {
                        s.folders(prov.as_deref(), parent.as_deref(), &q, cursor)
                            .await
                    });
                }
                if folder.is_none() && self.children.total == Some(0) {
                    ui.weak(tr("没有可浏览的音乐文件夹。"));
                }
                if let Some(id) = folder.as_ref() {
                    ui.separator();
                    self.recordings(ui, svc, providers);
                    if self.tracks.request(ui, "加载更多曲目") {
                        let (s, prov, id, q, cursor) = (
                            svc.server.clone(),
                            self.provider.clone(),
                            id.clone(),
                            self.search.clone(),
                            self.tracks.cursor.clone(),
                        );
                        self.tracks.load(ui, svc, async move {
                            s.folder_recordings(prov.as_deref(), &id, &q, cursor).await
                        });
                    }
                    if self.tracks.total == Some(0) {
                        ui.weak(tr("没有找到相关曲目"));
                    }
                }
            });
        if let Some(f) = open {
            self.path.push(f);
            self.change_folder();
        }
    }
    fn recordings(&mut self, ui: &mut egui::Ui, svc: Services<'_>, providers: &Providers) {
        let mut play = None;
        for (i, t) in self.tracks.rows.iter().enumerate() {
            let title = t
                .title
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&t.display_name);
            ui.push_id(&t.source_track_id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(playable(t), egui::Button::new(title).frame(false))
                        .clicked()
                    {
                        play = Some(i);
                    }
                    if let Some(artists) = &t.artists {
                        ui.weak(artists.join(" / "));
                    }
                    if self.provider.is_none() {
                        ui.weak(providers.label(&t.provider_instance_id));
                    }
                    if playable(t) {
                        if let Some(ms) = t.duration_ms {
                            ui.weak(format!("{}:{:02}", ms / 60_000, ms / 1000 % 60));
                        }
                        if ui.small_button(tr("加入队列")).clicked() {
                            self.message = svc.player.enqueue(queue(t)).err();
                        }
                    } else {
                        ui.weak(tr("不可播放"));
                    }
                })
            });
        }
        if let Some(i) = play {
            let position = self.tracks.rows[..i].iter().filter(|r| playable(r)).count();
            let q = self
                .tracks
                .rows
                .iter()
                .filter(|r| playable(r))
                .map(queue)
                .collect();
            self.message = svc.player.set_queue(q, position).err();
        }
    }
    fn play_all(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let Some(id) = self.path.last().map(|f| f.category_id.clone()) else {
            return;
        };
        let (s, provider, q) = (
            svc.server.clone(),
            self.provider.clone(),
            self.search.clone(),
        );
        let mut tracks: Vec<_> = self
            .tracks
            .rows
            .iter()
            .filter(|r| playable(r))
            .map(queue)
            .collect();
        let mut cursor = self.tracks.cursor.clone();
        self.all = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            let mut seen = HashSet::new();
            while let Some(c) = cursor {
                if !seen.insert(c.token.clone()) {
                    return Err("Repeated MusicFS folder cursor.".into());
                }
                let p = s
                    .folder_recordings(provider.as_deref(), &id, &q, Some(c))
                    .await?;
                tracks.extend(p.items.iter().filter(|r| playable(r)).map(queue));
                cursor = p.next;
            }
            Ok(tracks)
        }));
    }
    fn change_folder(&mut self) {
        self.search.clear();
        self.draft.clear();
        self.reload();
    }
    fn reload(&mut self) {
        self.children = Pages::default();
        self.tracks = Pages::default();
        self.all = None;
        self.message = None;
    }
}
fn label<'a>(f: &'a Folder, providers: &'a Providers) -> &'a str {
    if f.parent_id.is_none() {
        providers
            .root_label(&f.provider_instance_id, &f.root_id)
            .unwrap_or(tr("音乐目录"))
    } else {
        &f.name
    }
}
fn playable(t: &Recording) -> bool {
    t.availability == "available"
}
fn queue(t: &Recording) -> Track {
    // 文件夹对应具体文件。用来源身份播放，以免曲库的默认版本选择到另一个目录。
    Track {
        id: t.source_track_id.clone(),
        source_only: true,
        source_track_id: Some(t.source_track_id.clone()),
        forced_source_track_id: None,
        title: t
            .title
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| t.display_name.clone()),
        artists: t.artists.clone().unwrap_or_default(),
        album: t.album.clone(),
        cover_object_id: t.cover_object_id.clone(),
        duration_ms: t.duration_ms,
    }
}
