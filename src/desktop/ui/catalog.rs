mod collection;
mod new_track;
mod queue;
mod toolbar;
use crate::i18n::tr;
pub(crate) use collection::category_cards as card_grid;
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
    pub search_draft: String,
    pub search: String,
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
    pub fn search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.search_draft)
                    .hint_text(tr("搜索歌曲、歌手或专辑"))
                    .desired_width((ui.available_width() - 95.0).clamp(120.0, 460.0)),
            );
            if self.focus_search {
                field.request_focus();
                self.focus_search = false;
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if style::primary(ui, tr("搜索"), !self.search_draft.trim().is_empty()).clicked()
                || enter
            {
                self.search = self.search_draft.trim().into();
                self.reset_tracks();
            }
        });
        ui.add_space(10.0);
    }

    pub fn open_credit(&mut self, tab: CategoryTab, id: String, name: String) {
        if matches!(self.query, Some(TrackQuery::AvailableSources { .. })) {
            self.providers.selected = None;
        }
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
        ui.horizontal_wrapped(|ui| {
            if self.providers.select(ui) {
                self.reset_categories();
            }
            if self.tabs == CategoryTab::Album && self.category.is_none() {
                self.album_filters(ui);
            }
            if self.tabs != CategoryTab::Folder {
                self.list_search(ui);
            }
            if self.tabs == CategoryTab::Album && self.category.is_none() {
                self.matching.queue_albums(
                    ui,
                    svc,
                    self.providers.selected.as_deref(),
                    &self.list_query,
                );
            }
        });
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
                    provider: self.providers.selected.clone(),
                    query: self.list_query.clone(),
                });
            }
        }
        self.categories.poll();
        let mut load = !self.categories.started;
        ui.horizontal(|ui| {
            if ui.button(tr("刷新分类")).clicked() {
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
            let provider = self.providers.selected.clone();
            let query = self.list_query.clone();
            let unscraped_only = self.unscraped_only;
            self.categories.load(ui.ctx(), svc.runtime, async move {
                server
                    .filtered_categories(
                        kind,
                        provider.as_deref(),
                        &query,
                        cursor.as_deref(),
                        unscraped_only,
                    )
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
