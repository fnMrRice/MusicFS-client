use super::{
    data::{Pages, Services, TrackQuery},
    navigation::Screen,
    table, SignedUi,
};
use crate::i18n::tr;
use eframe::egui::{self, RichText};

impl SignedUi {
    pub(super) fn page(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        let screen = self.screen.clone();
        let query = match screen {
            Screen::Library => {
                let Some(query) = self.catalog.library(ui, svc) else {
                    return;
                };
                query
            }
            Screen::Search => {
                self.catalog.search_bar(ui);
                if self.catalog.search.is_empty() {
                    ui.weak(tr("输入关键词并按回车搜索音乐库。"));
                    return;
                }
                TrackQuery::Library(self.catalog.search.clone())
            }
            Screen::Favorites => TrackQuery::Favorites,
            Screen::History => {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.history_type, String::new(), tr("音乐"));
                    for t in self.browse.types().iter().filter(|t| t.separate_history) {
                        ui.selectable_value(&mut self.history_type, t.id.clone(), tr(&t.name));
                    }
                });
                ui.separator();
                if !self.history_type.is_empty() {
                    let q = egui::ScrollArea::vertical()
                        .id_salt("work-history")
                        .show(ui, |ui| {
                            self.browse
                                .history(ui, svc.runtime, svc.server, &self.history_type)
                        })
                        .inner;
                    self.play_request(svc, q);
                    return;
                }
                TrackQuery::History
            }
            Screen::Playlists => {
                self.playlists(ui, svc);
                return;
            }
            Screen::Playlist(id) => {
                let Some(p) = self.lists.items.iter().find(|p| p.id == id) else {
                    if self.lists.busy() {
                        ui.spinner();
                    } else if let Some(error) = &self.lists.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                        if ui.button(tr("重试读取歌单")).clicked() {
                            self.lists = Pages::default();
                        }
                    } else if self.lists.next.is_some() {
                        self.load_lists(ui.ctx(), svc);
                        ui.spinner();
                    } else {
                        ui.weak(tr("此歌单已删除或无法访问。"));
                    }
                    if ui.button(tr("返回歌单列表")).clicked() {
                        self.navigate(Screen::Playlists);
                    }
                    return;
                };
                if p.stale {
                    ui.horizontal(|ui| {
                        ui.weak(tr("歌单有未应用的更改。"));
                        if ui
                            .add_enabled(
                                !self.actions.busy(),
                                egui::Button::new(tr("更新歌单内容")),
                            )
                            .clicked()
                        {
                            self.actions.rebuild(ui, svc, p);
                        }
                    });
                }
                TrackQuery::Playlist {
                    id,
                    revision: p.snapshot_revision,
                }
            }
            Screen::Recommendations => {
                let q = self
                    .browse
                    .recommendations(ui, svc, &mut self.catalog.artwork);
                self.play_request(svc, q);
                return;
            }
            Screen::Works(id) => {
                let q = egui::ScrollArea::vertical()
                    .id_salt(("works", &id))
                    .show(ui, |ui| self.browse.works(ui, svc.runtime, svc.server, &id))
                    .inner;
                self.play_request(svc, q);
                return;
            }
            Screen::Settings => return,
        };
        let source = self.player.current_source_id();
        table::tracks(
            ui,
            svc,
            &mut self.catalog,
            &mut self.actions,
            query,
            &self.lists.items,
            source,
            self.player.status(),
        );
    }
    fn play_request(
        &mut self,
        svc: Services<'_>,
        request: Option<(Vec<musicfs_player_core::Track>, usize)>,
    ) {
        if let Some((tracks, index)) = request {
            if let Err(error) = svc.player.set_queue(tracks, index) {
                self.actions.notice = Some((error, true));
            }
        }
    }
    fn playlists(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        ui.horizontal(|ui| {
            ui.strong(tr("{0} 个歌单").replace("{0}", &self.lists.items.len().to_string()));
            if ui.button(tr("刷新歌单")).clicked() {
                self.lists = Pages::default();
            }
            if self.lists.busy() {
                ui.spinner();
            }
        });
        ui.separator();
        if let Some(error) = &self.lists.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.lists.started
            && !self.lists.busy()
            && self.lists.error.is_none()
            && self.lists.items.is_empty()
        {
            ui.weak(tr("暂无歌单。可从曲目右侧的「＋」新建歌单。"));
        }
        let mut selected = None;
        let mut more = false;
        egui::ScrollArea::vertical()
            .id_salt("all-playlists")
            .show(ui, |ui| {
                for p in &self.lists.items {
                    if ui
                        .add_sized(
                            [ui.available_width(), 42.0],
                            egui::Button::new(RichText::new(&p.name).size(15.0)),
                        )
                        .clicked()
                    {
                        selected = Some(p.id.clone());
                    }
                }
                if self.lists.next.is_some() || self.lists.error.is_some() {
                    more = ui
                        .add_enabled(!self.lists.busy(), egui::Button::new(tr("继续读取歌单")))
                        .clicked();
                }
            });
        if more {
            self.load_lists(ui.ctx(), svc);
        }
        if let Some(id) = selected {
            self.navigate(Screen::Playlist(id));
        }
    }
}
