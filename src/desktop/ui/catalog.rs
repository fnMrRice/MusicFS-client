mod collection;
mod new_track;
mod queue;
mod search;
mod toolbar;
use crate::i18n::tr;
pub(crate) use collection::category_cards as card_grid;
pub(super) use search::SearchAction;
#[path = "providers.rs"]
pub(super) mod providers;
#[path = "versions.rs"]
mod versions;
use super::data::{queue_track, CategoryTab, Pages, Services, TrackQuery};
use super::style;
use super::Artwork;
use crate::task::Task;
use eframe::egui::{self, RichText};
use musicfs_player_core::Track as QueueTrack;
use musicfs_server_adapter::{Category, Track};

#[cfg(test)]
#[path = "../../../tests/native/catalog_search.rs"]
mod search_tests;

#[derive(Default)]
pub struct Catalog {
    pub matching: super::matching::Matching,
    pub album_merge: super::album_merge::AlbumMerge,
    folders: super::folders::Folders,
    pub versions: versions::Versions,
    pub artwork: Artwork,
    pub tabs: CategoryTab,
    album_list: bool,
    unscraped_only: bool,
    pub providers: providers::Providers,
    pub category: Option<(String, String)>,
    pub categories: Pages<Category>,
    pub tracks: Pages<Track>,
    query: Option<TrackQuery>,
    complete: Option<(bool, Task<Vec<QueueTrack>>)>,
    creation: Option<new_track::NewTrack>,
    feedback: Option<String>,
    list_draft: String,
    list_query: String,
    search: search::LibrarySearch,
    pub focus_search: bool,
}

impl Catalog {
    pub fn reset_tracks(&mut self) {
        self.matching.clear_selection();
        self.versions = Default::default();
        self.tracks = Pages::default();
        self.query = None;
        self.complete = None;
    }
    pub fn reset_categories(&mut self) {
        self.category = None;
        self.categories = Pages::default();
        self.reset_tracks();
    }
    pub fn open_credit(&mut self, tab: CategoryTab, id: String, name: String) {
        self.tabs = tab;
        self.list_draft.clear();
        self.list_query.clear();
        self.reset_categories();
        self.category = Some((id, name));
    }

    pub fn library(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> Option<TrackQuery> {
        self.category_tabs(ui);
        self.providers.sync(ui, svc);
        if self.tabs == CategoryTab::All {
            return Some(TrackQuery::AvailableSources {
                query: self.list_query.clone(),
            });
        }
        self.providers.status(ui);
        if self.tabs == CategoryTab::Folder {
            self.folders.show(ui, svc, &self.providers);
            return None;
        }
        let kind = self.tabs.kind()?;
        if let Some((id, name)) = self.category.clone() {
            let mut back = false;
            ui.horizontal(|ui| {
                back = ui.link(tr("返回分类")).clicked();
                ui.weak("/");
                ui.strong(if name.is_empty() {
                    tr("未知{0}").replace("{0}", self.tabs.label())
                } else {
                    name
                });
            });
            if back {
                self.list_draft.clear();
                self.list_query.clear();
                self.category = None;
                self.reset_tracks();
            } else {
                return Some(TrackQuery::Category {
                    tab: self.tabs,
                    id,
                    query: self.list_query.clone(),
                });
            }
        }
        self.categories.poll();
        ui.horizontal_wrapped(|ui| {
            if self.tabs == CategoryTab::Album {
                self.album_filters(ui);
                self.matching.queue_albums(ui, svc, None, &self.list_query);
            }
            if self.categories.busy() {
                ui.spinner();
            }
            if self.list_search(ui, tr("刷新分类")) {
                self.categories = Pages::default();
            }
        });
        let mut load = !self.categories.started;
        ui.add_space(6.0);
        if let Some(error) = self.categories.error.as_ref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button(tr("重试加载分类")).clicked() {
                load = true;
            }
        } else if self.categories.started
            && !self.categories.busy()
            && self.categories.items.is_empty()
        {
            ui.weak(tr("音乐库中暂无{0}。").replace("{0}", self.tabs.label()));
        }
        self.artwork.poll(ui.ctx());
        egui::ScrollArea::vertical()
            .id_salt("category-list")
            .show(ui, |ui| {
                if let Some((id, name)) = self.collection(ui, svc) {
                    self.list_draft.clear();
                    self.list_query.clear();
                    self.category = Some((id, name));
                }
                if self.categories.next.is_some()
                    && ui
                        .add_enabled(
                            !self.categories.busy(),
                            egui::Button::new(tr("加载更多分类")),
                        )
                        .clicked()
                {
                    load = true;
                }
            });
        if load {
            let server = svc.server.clone();
            let cursor = self.categories.next.clone();
            let query = self.list_query.clone();
            let unscraped_only = self.unscraped_only;
            self.categories.load(ui.ctx(), svc.runtime, async move {
                server
                    .filtered_categories(kind, None, &query, cursor.as_deref(), unscraped_only)
                    .await
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
}
