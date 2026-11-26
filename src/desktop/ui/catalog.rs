mod album_cover;
mod cache;
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
    album_cover: album_cover::AlbumCover,
    pub tabs: CategoryTab,
    album_list: bool,
    unscraped_only: bool,
    pub providers: providers::Providers,
    pub category: Option<(String, String)>,
    pub categories: Pages<Category>,
    pub tracks: Pages<Track>,
    cache: cache::LibraryCache,
    source_revision: Option<u64>,
    pub(super) playlist: super::playlist_tracks::LocalTracks,
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
        self.playlist.reset_pages();
        self.query = None;
        self.complete = None;
    }
    fn select_tab(&mut self, tab: CategoryTab) {
        self.leave_lists();
        self.tabs = tab;
        self.list_draft.clear();
        self.list_query.clear();
        self.category = None;
        self.album_cover.leave();
    }
    pub fn open_credit(&mut self, tab: CategoryTab, id: String, name: String) {
        self.select_tab(tab);
        self.select_categories();
        self.category = Some((id, name));
    }

    pub fn library(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> Option<TrackQuery> {
        let revision = svc.activity.revisions().capabilities;
        if self
            .source_revision
            .replace(revision)
            .is_some_and(|old| old != revision)
        {
            self.invalidate_library();
            self.providers.refresh();
        }
        self.album_cover.poll(&mut self.artwork);
        self.artwork.poll(ui.ctx());
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
                back = style::link(ui, tr("返回分类")).clicked();
                ui.weak("/");
                ui.strong(if name.is_empty() {
                    tr("未知{0}").replace("{0}", self.tabs.label())
                } else {
                    name
                });
                if self.tabs == CategoryTab::Album {
                    let known = self.categories.items.iter().find(|album| album.id == id);
                    self.album_cover
                        .show(ui, svc, &mut self.artwork, &id, known);
                }
            });
            if back {
                self.list_draft.clear();
                self.list_query.clear();
                self.category = None;
                self.album_cover.leave();
                self.leave_tracks();
                self.categories.resume(cache::MAX_AGE);
            } else {
                return Some(TrackQuery::Category {
                    tab: self.tabs,
                    id,
                    query: self.list_query.clone(),
                });
            }
        }
        let refresh = ui
            .horizontal_wrapped(|ui| {
                if self.tabs == CategoryTab::Album {
                    self.album_filters(ui);
                    if self.matching.album_queue_status(ui, svc) {
                        self.invalidate_library();
                    }
                }
                if self.categories.busy() {
                    ui.spinner();
                }
                self.list_search(ui, tr("刷新分类"), |catalog, ui| {
                    if catalog.tabs == CategoryTab::Album {
                        catalog
                            .matching
                            .queue_albums(ui, svc, None, &catalog.list_query);
                    }
                })
            })
            .inner;
        self.select_categories();
        if refresh {
            self.categories = Pages::default();
            self.artwork.retry_failed();
        }
        self.categories.poll();
        let mut load = !self.categories.started;
        ui.add_space(6.0);
        if let Some(error) = self.categories.error.as_ref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if style::button(ui, tr("重试加载分类")).clicked() {
                load = true;
            }
        } else if self.categories.started
            && !self.categories.busy()
            && self.categories.items.is_empty()
            && self.categories.next.is_none()
        {
            ui.weak(tr("音乐库中暂无{0}。").replace("{0}", self.tabs.label()));
        }
        egui::ScrollArea::vertical()
            .id_salt("category-list")
            .show(ui, |ui| {
                if let Some((id, name)) = self.collection(ui, svc) {
                    self.list_draft.clear();
                    self.list_query.clear();
                    self.category = Some((id, name));
                    ui.ctx().request_repaint();
                }
                if self.category.is_none()
                    && self.categories.can_load_more()
                    && super::page_end_visible(ui)
                {
                    load = true;
                }
            });
        if load {
            let server = svc.server.clone();
            let cursor = self.categories.request_cursor();
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
        self.select_tracks(query);
        if query.is_playlist() {
            if let Some(page) = self.tracks.take_page() {
                self.playlist.append(page, &mut self.tracks.items);
                if self.playlist.tools.sorted() {
                    self.versions.clear_expanded();
                }
            }
        } else if self.tracks.poll() {
            self.matching.clear_selection();
            self.versions.clear_expanded();
        }
        if !self.tracks.started
            || (query.is_playlist() && self.playlist.tools.active() && self.tracks.can_load_more())
        {
            self.more(ui, svc);
        }
    }
    pub fn more(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let Some(query) = self.query.clone() else {
            return;
        };
        let cursor = self.tracks.request_cursor();
        let server = svc.server.clone();
        self.tracks.load(ui.ctx(), svc.runtime, async move {
            query.fetch(&server, cursor.as_deref()).await
        });
    }
}
