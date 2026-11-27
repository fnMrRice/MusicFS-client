mod cards;
mod header;
mod recordings;

use super::{catalog::providers::Providers, data::Services};
use crate::ui::style;
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
                    .is_some_and(|c| c.token.is_empty() || !self.seen.insert(c.token.clone())) =>
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
    fn request(&mut self, ui: &mut egui::Ui) -> bool {
        if self.task.is_some() {
            ui.spinner();
            false
        } else if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
            if style::button(ui, tr("重试")).clicked() {
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
            self.cursor.is_some() && super::page_end_visible(ui)
        }
    }
}
#[derive(Default)]
pub(super) struct Folders {
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
        if let Some(depth) = header::ancestors(ui, &self.path, providers) {
            self.path.truncate(depth);
            self.change_folder();
        }
        self.children.poll();
        self.tracks.poll();
        self.toolbar(ui, providers);
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
        let open = egui::ScrollArea::vertical()
            .id_salt(("catalog-folders", folder.as_deref(), &self.search))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut open = None;
                if folder.is_none() || self.search.is_empty() {
                    open = self.folder_grid(ui, svc, providers, folder.as_deref());
                }
                if let Some(id) = folder.as_ref() {
                    ui.horizontal(|ui| {
                        header::section(ui, tr("目录曲目"), self.tracks.total);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let enabled = self.all.is_none()
                                && (self.tracks.rows.iter().any(playable)
                                    || self.tracks.cursor.is_some());
                            if style::primary(ui, tr("播放全部"), enabled).clicked() {
                                self.play_all(ui, svc);
                            }
                            if self.all.is_some() {
                                ui.spinner();
                            }
                        });
                    });
                    self.recordings(ui, svc, providers);
                    if self.tracks.request(ui) {
                        let (s, id, q, cursor) = (
                            svc.server.clone(),
                            id.clone(),
                            self.search.clone(),
                            self.tracks.cursor.clone(),
                        );
                        self.tracks.load(ui, svc, async move {
                            s.folder_recordings(None, &id, &q, cursor).await
                        });
                    }
                    if self.tracks.total == Some(0) {
                        let (icon, message) = if self.search.is_empty() {
                            ("music", "此文件夹中暂无曲目")
                        } else {
                            ("search", "没有找到相关曲目")
                        };
                        header::empty(ui, icon, tr(message));
                    }
                }
                open
            })
            .inner;
        if let Some(index) = open {
            self.path.push(self.children.rows[index].clone());
            self.change_folder();
            ui.ctx().request_repaint();
        }
    }
    fn folder_grid(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        providers: &Providers,
        parent: Option<&str>,
    ) -> Option<usize> {
        if parent.is_some() && !self.children.rows.is_empty() {
            ui.horizontal(|ui| header::section(ui, tr("子文件夹"), self.children.total));
        }
        let open = cards::grid(ui, &self.children.rows, providers);
        if self.children.request(ui) {
            let server = svc.server.clone();
            let parent = parent.map(str::to_owned);
            let query = self.search.clone();
            let cursor = self.children.cursor.clone();
            self.children.load(ui, svc, async move {
                server
                    .folders(None, parent.as_deref(), &query, cursor)
                    .await
            });
        }
        if parent.is_none() && self.children.total == Some(0) {
            let (icon, message) = if self.search.is_empty() {
                ("folder", "没有可浏览的音乐文件夹。")
            } else {
                ("search", "没有找到相关文件夹")
            };
            header::empty(ui, icon, tr(message));
        }
        if parent.is_some() && !self.children.rows.is_empty() {
            ui.add_space(12.);
        }
        open
    }
    fn recordings(&mut self, ui: &mut egui::Ui, svc: Services<'_>, providers: &Providers) {
        use recordings::Action;
        let rows = &self.tracks.rows;
        self.message = match recordings::list(ui, rows, providers) {
            Some(Action::Play(i)) => {
                let position = rows[..i].iter().filter(|r| playable(r)).count();
                let q = rows.iter().filter(|r| playable(r)).map(queue).collect();
                svc.player.set_queue(q, position).err()
            }
            Some(Action::Enqueue(i)) => svc.player.enqueue(queue(&rows[i])).err(),
            None => return,
        };
    }
    fn play_all(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let Some(id) = self.path.last().map(|f| f.category_id.clone()) else {
            return;
        };
        let s = svc.server.clone();
        let q = self.search.clone();
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
                let p = s.folder_recordings(None, &id, &q, Some(c)).await?;
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
        online_cover_path: None,
        duration_ms: t.duration_ms,
    }
}
