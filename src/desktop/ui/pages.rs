use super::{
    data::{Pages, Services, TrackQuery},
    navigation::Screen,
    style, table, SignedUi,
};
use crate::i18n::tr;
use eframe::egui;

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
            Screen::Search => match self.catalog.search_page(ui, svc) {
                super::catalog::SearchAction::Tracks(query) => query,
                super::catalog::SearchAction::OpenCategory => {
                    self.navigate(Screen::Library);
                    return;
                }
                super::catalog::SearchAction::Stay => return,
            },
            Screen::Favorites => TrackQuery::Favorites,
            Screen::History => {
                ui.horizontal_wrapped(|ui| {
                    style::selectable_value(ui, &mut self.history_type, String::new(), tr("音乐"));
                    for t in self.browse.types().iter().filter(|t| t.separate_history) {
                        style::selectable_value(
                            ui,
                            &mut self.history_type,
                            t.id.clone(),
                            tr(&t.name),
                        );
                    }
                });
                ui.separator();
                if !self.history_type.is_empty() {
                    let q = egui::ScrollArea::vertical()
                        .id_salt("work-history")
                        .show(ui, |ui| {
                            self.browse.history(
                                ui,
                                svc,
                                &mut self.catalog.artwork,
                                &self.history_type,
                            )
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
            Screen::SourcePlaylist(key) => {
                self.playlist_operations(ui, svc);
                let outcome = self.playlists.source_detail(
                    ui, svc, &mut self.catalog.artwork, &self.lists, &key,
                );
                if outcome.more_local {
                    self.load_lists(ui.ctx(), svc);
                }
                self.play_request(svc, outcome.play);
                return;
            }
            Screen::Playlist(id) => {
                let Some(p) = self.lists.items.iter().find(|p| p.id == id) else {
                    if self.lists.busy() {
                        ui.spinner();
                    } else if let Some(error) = &self.lists.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                        if style::button(ui, tr("重试读取歌单")).clicked() {
                            self.lists = Pages::default();
                            ui.ctx().request_repaint();
                        }
                    } else if self.lists.next.is_some() {
                        self.load_lists(ui.ctx(), svc);
                        ui.spinner();
                    } else {
                        ui.weak(tr("此歌单已删除或无法访问。"));
                    }
                    if style::button(ui, tr("返回歌单列表")).clicked() {
                        self.navigate(Screen::Playlists);
                    }
                    return;
                };
                self.local_playlists.detail(ui, p);
                if p.stale {
                    ui.horizontal(|ui| {
                        ui.weak(tr("歌单有未应用的更改。"));
                        if style::add_button(
                            ui,
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
                let q = self.browse.works(ui, svc, &mut self.catalog.artwork, &id);
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
    pub(super) fn play_request(
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
}
