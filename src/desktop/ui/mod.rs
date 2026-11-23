use crate::i18n::tr;
mod actions;
mod catalog;
mod data;
mod favorites;
mod folders;
mod login;
mod nav_icons;
mod navigation;
mod settings;
mod style;
mod table;

#[cfg(test)]
#[path = "../../../tests/native/playlist_pages.rs"]
mod playlist_pages;

use crate::{
    browse::BrowseUi,
    model::{DesktopState, Preferences},
    player_ui::PlayerUi,
};
use actions::Actions;
use catalog::Catalog;
use data::{Pages, Services, TrackQuery};
use eframe::egui::{self, Align, Layout, RichText};
use musicfs_player_core::{FadeSettings, PlayerCommand};
use musicfs_server_adapter::Playlist;
use navigation::Screen;

#[derive(Default)]
pub struct DesktopUi {
    login: login::Login,
    activity: Option<musicfs_server_adapter::activity::ActivityStream>,
    signed: Option<SignedUi>,
    theme: String,
    initialized: bool,
}

#[derive(Default)]
struct SignedUi {
    settings: settings::Settings,
    screen: Screen,
    player: PlayerUi,
    browse: BrowseUi,
    catalog: Catalog,
    lists: Pages<Playlist>,
    actions: Actions,
    works_open: bool,
    mine_closed: bool,
    lists_closed: bool,
    history_type: String,
}

enum HostAction {
    Reauthenticate,
    Audio(crate::model::audio::AudioAction),
    Logout,
    Language(String),
    Theme(String),
    Fades(FadeSettings),
}

impl DesktopUi {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut DesktopState) {
        crate::i18n::apply(&state.prefs.language);
        if !self.initialized {
            self.login.endpoint = if state.prefs.endpoint.is_empty() {
                "http://127.0.0.1:8080".into()
            } else {
                state.prefs.endpoint.clone()
            };
            self.initialized = true;
        }
        if self.theme != state.prefs.theme || self.theme.is_empty() {
            self.theme = state.prefs.theme.clone();
            style::apply(ctx, &self.theme);
        }
        let Some(connection) = &state.connection else {
            self.signed = None;
            self.activity = None;
            self.login.show(ctx, state);
            return;
        };
        let ui = self.signed.get_or_insert_with(SignedUi::default);
        let svc = Services {
            runtime: &state.runtime,
            activity: self.activity.get_or_insert_with(|| {
                let ctx = ctx.clone();
                connection.server.listen_activity(move || ctx.request_repaint())
            }),
            server: &connection.server,
            player: &connection.player,
        };
        match ui.show(ctx, svc, &state.prefs, state.busy, state.error.as_deref()) {
            Some(HostAction::Audio(action)) => match state.prefs.audio.apply(connection, action) {
                Ok(()) => state.save_preferences(),
                Err(error) => state.error = Some(error),
            },
            Some(HostAction::Logout) => state.disconnect(ctx),
            Some(HostAction::Reauthenticate) => state.reauthenticate(ctx),
            Some(HostAction::Language(language)) => {
                state.prefs.language = language;
                state.save_preferences();
                ctx.request_repaint();
            }
            Some(HostAction::Theme(theme)) => {
                state.prefs.theme = theme;
                state.save_preferences();
            }
            Some(HostAction::Fades(settings)) => {
                match svc.player.command(PlayerCommand::SetFades { settings }) {
                    Ok(_) => {
                        state.prefs.fades = settings;
                        state.save_preferences();
                    }
                    Err(error) => state.error = Some(error),
                }
            }
            None => {}
        }
    }
}

impl SignedUi {
    fn load_lists(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let cursor = self.lists.next.clone();
        let server = svc.server.clone();
        self.lists.load(ctx, svc.runtime, async move {
            server.playlists(cursor.as_deref()).await
        });
    }
    fn navigate(&mut self, screen: Screen) {
        if self.screen != screen {
            self.catalog.reset_tracks();
            self.actions.notice = None;
        }
        if matches!(screen, Screen::Search) {
            self.catalog.focus_search = true;
        }
        self.screen = screen;
    }
    fn title(&self) -> String {
        match &self.screen {
            Screen::Playlist(id) => self
                .lists
                .items
                .iter()
                .find(|p| p.id == *id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| tr("歌单详情").into()),
            Screen::Works(id) => self
                .browse
                .types()
                .iter()
                .find(|t| t.id == *id)
                .map(|t| tr(&t.name).to_owned())
                .unwrap_or_else(|| tr("作品库").into()),
            screen => screen.label().into(),
        }
    }
    fn show(
        &mut self,
        ctx: &egui::Context,
        svc: Services<'_>,
        prefs: &Preferences,
        busy: bool,
        error: Option<&str>,
    ) -> Option<HostAction> {
        self.browse.poll_types(ctx, svc.runtime, svc.server);
        let updated = self.actions.poll();
        self.actions.favorites.sync(ctx, svc.runtime, svc.server);
        if updated.playlists {
            self.lists = Pages::default();
        }
        if updated.favorites && self.screen == Screen::Favorites {
            self.catalog.reset_tracks();
        }
        self.lists.poll();
        if !self.lists.started {
            self.load_lists(ctx, svc);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.navigate(Screen::Search);
        }
        let changed_volume = self.player.show(
            ctx,
            svc.player,
            svc.server,
            svc.runtime,
            prefs.audio.volume,
            prefs.audio.last_volume,
        );
        self.sidebar(ctx, svc);
        let dark = ctx.style().visuals.dark_mode;
        let mut action =
            changed_volume.map(|v| HostAction::Audio(crate::model::audio::AudioAction::Volume(v)));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(style::background(dark))
                    .inner_margin(egui::Margin::symmetric(25, 17)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(self.title());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr("账号与设置")).clicked() {
                            self.navigate(Screen::Settings);
                        }
                        ui.menu_button(tr("换肤"), |ui| {
                            for (value, label) in themes() {
                                if ui
                                    .selectable_label(prefs.theme == value, tr(label))
                                    .clicked()
                                {
                                    action = Some(HostAction::Theme(value.into()));
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.available_width() > 120.0 {
                            ui.colored_label(style::muted(dark), tr("● 已连接"))
                                .on_hover_text(&svc.server.info.endpoint);
                        }
                    });
                });
                ui.add_space(12.0);
                if let Some((msg, is_error)) = &self.actions.notice {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            if *is_error {
                                ui.visuals().error_fg_color
                            } else {
                                style::muted(dark)
                            },
                            msg,
                        );
                        if msg.contains("HTTP 401") && ui.button(tr("重新登录")).clicked() {
                            action = Some(HostAction::Reauthenticate);
                        }
                    });
                }
                if let Some(error) = error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                if self.screen == Screen::Settings {
                    self.settings.show(ui, svc, prefs, busy, &mut action);
                } else {
                    self.page(ui, svc);
                }
            });
        if self.actions.show_editor(ctx, svc, &self.lists) {
            self.load_lists(ctx, svc);
        }
        action
    }
    fn page(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
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
                egui::ScrollArea::vertical()
                    .id_salt("recommendations")
                    .show(ui, |ui| {
                        self.browse.recommendations(ui, svc.runtime, svc.server)
                    });
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
        let source = self
            .player
            .current_track()
            .and_then(|t| t.source_track_id.as_deref());
        table::tracks(
            ui,
            svc,
            &mut self.catalog,
            &mut self.actions,
            query,
            &self.lists.items,
            source,
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

fn themes() -> [(&'static str, &'static str); 3] {
    [("dark", "深色"), ("light", "浅色"), ("system", "跟随系统")]
}
