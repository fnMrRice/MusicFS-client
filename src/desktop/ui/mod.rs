use crate::i18n::tr;
mod actions;
mod album_merge;
mod artwork;
mod catalog;
mod data;
mod favorites;
mod folders;
mod login;
mod matching;
mod navigation;
mod pages;
mod playlists;
mod settings;
mod shell;
pub(crate) mod style;
mod table;

#[cfg(test)]
#[path = "../../../tests/native/playlist_pages.rs"]
mod playlist_pages;

#[cfg(test)]
#[path = "../../../tests/native/library_autoload.rs"]
mod library_autoload;

use crate::{
    browse::BrowseUi,
    model::{DesktopState, Preferences},
    player_ui::{BarChange, PlayerUi},
};
use actions::Actions;
pub(crate) use artwork::Artwork;
pub(crate) use catalog::card_grid;
use catalog::Catalog;
use data::Pages;
pub(crate) use data::{resolve_online_queue_track, Services};
use eframe::egui;
use musicfs_player_core::{FadeSettings, PlaybackMode, PlayerCommand};
use musicfs_server_adapter::Playlist;
use navigation::Screen;

// 在当前滚动容器里为列表末尾留出位置，短列表和滚动到底部时都会触发续页。
pub(crate) fn page_end_visible(ui: &mut egui::Ui) -> bool {
    let (_, end) = ui.allocate_space(egui::vec2(1., 12.));
    ui.is_rect_visible(end)
}

#[derive(Default)]
pub struct DesktopUi {
    login: login::Login,
    activity: Option<musicfs_server_adapter::activity::ActivityStream>,
    signed: Option<SignedUi>,
    theme: String,
    accent: String,
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
    playlists: playlists::Management,
    local_playlists: playlists::local::LocalPlaylists,
    actions: Actions,
    works_closed: bool,
    mine_closed: bool,
    lists_closed: bool,
    history_type: String,
    shell: shell::Shell,
}

enum HostAction {
    Reauthenticate,
    Audio(crate::model::audio::AudioAction),
    Logout,
    Language(String),
    Theme(String),
    Accent(String),
    Fades(FadeSettings),
    PlaybackMode(PlaybackMode),
}

impl DesktopUi {
    pub fn system_state(&self) -> crate::system::SystemState {
        self.signed
            .as_ref()
            .map(|ui| ui.player.system_state())
            .unwrap_or_default()
    }

    pub fn refresh_player(&mut self) {
        if let Some(ui) = &mut self.signed {
            ui.player.refresh();
        }
    }

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
        if self.theme != state.prefs.theme
            || self.accent != state.prefs.accent
            || self.theme.is_empty()
        {
            self.theme = state.prefs.theme.clone();
            self.accent = state.prefs.accent.clone();
            style::apply(ctx, &self.theme, &self.accent);
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
                connection
                    .server
                    .listen_activity(move || ctx.request_repaint())
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
            Some(HostAction::Accent(accent)) => {
                state.prefs.accent = accent;
                state.save_preferences();
                ctx.request_repaint();
            }
            Some(HostAction::PlaybackMode(mode)) => {
                match svc.player.command(PlayerCommand::SetPlaybackMode { mode }) {
                    Ok(_) => {
                        state.prefs.playback_mode = mode;
                        state.save_preferences();
                        ui.player.refresh();
                        ctx.request_repaint();
                    }
                    Err(error) => state.error = Some(error),
                }
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
            self.shell.history.push(self.screen.clone());
        }
        self.set_screen(screen);
    }
    fn back(&mut self, wide: bool) {
        if self.screen == Screen::Playlists && self.playlists.back() {
            return;
        }
        if let Some(screen) = self.shell.history.pop() {
            self.set_screen(screen);
        } else {
            self.shell.navigation = Some(!self.shell.navigation.unwrap_or(wide));
            self.works_closed = false;
        }
    }
    fn set_screen(&mut self, screen: Screen) {
        if self.screen != screen {
            self.catalog.reset_tracks();
            if self.screen == Screen::Playlists {
                self.playlists.leave();
            }
            self.actions.notice = None;
            if self.screen == Screen::Recommendations {
                self.browse.leave_recommendations();
            }
        }
        self.shell.navigation = None;
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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)) {
            self.back(ctx.screen_rect().width() > 850.);
        }
        let bar_change = self.player.show(
            ctx,
            svc.player,
            svc.server,
            svc.runtime,
            prefs.audio.volume,
            prefs.audio.last_volume,
        );
        if self
            .shell
            .navigation
            .unwrap_or(ctx.screen_rect().width() > 850.)
        {
            self.sidebar(ctx, svc);
        }
        let dark = ctx.style().visuals.dark_mode;
        let mut action = bar_change.map(|change| match change {
            BarChange::Volume(v) => HostAction::Audio(crate::model::audio::AudioAction::Volume(v)),
            BarChange::PlaybackMode(mode) => HostAction::PlaybackMode(mode),
        });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(style::background(dark))
                    .inner_margin(egui::Margin::symmetric(
                        if ctx.screen_rect().width() <= 850. {
                            18
                        } else {
                            30
                        },
                        0,
                    )),
            )
            .show(ctx, |ui| {
                self.header(ui, svc, prefs, &mut action);
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
                    if self.catalog.matching.take_plugin_settings() {
                        self.settings.open_plugins();
                        self.navigate(Screen::Settings);
                    }
                }
            });
        if self.actions.show_editor(ctx, svc, &self.lists) {
            self.load_lists(ctx, svc);
        }
        if self.local_playlists.dialogs(ctx, svc, &self.lists) {
            self.lists = Pages::default();
            if matches!(self.screen, Screen::Playlist(_)) {
                self.catalog.reset_tracks();
            }
            ctx.request_repaint();
        }
        action
    }
}

fn themes() -> [(&'static str, &'static str); 3] {
    [("dark", "深色"), ("light", "浅色"), ("system", "跟随系统")]
}
