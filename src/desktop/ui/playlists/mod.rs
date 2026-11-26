mod account;
pub(super) mod local;
mod operations;
mod sidebar;

use super::{
    data::{Pages, Services},
    navigation::Screen,
    style, SignedUi,
};
use crate::{
    browse::{resource::Resource, OnlinePlaylistDetail},
    i18n::tr,
    icons,
};
use eframe::egui;
use local::LocalAction;
use musicfs_server_adapter::online_playlists::{Directory, ImportedPlaylist, OnlinePlaylist};
use operations::Operations;

#[derive(Default, Copy, Clone, PartialEq, Eq)]
enum Tab {
    #[default]
    Local,
    Account,
    Imports,
}

#[derive(Default)]
pub(super) struct Management {
    tab: Tab,
    sources: Resource<Directory>,
    revision: Option<u64>,
    selected: String,
    identity: Option<(String, Option<String>, Option<String>)>,
    accounts: std::collections::HashMap<String, Pages<OnlinePlaylist>>,
    refresh_source: Option<String>,
    imports: Pages<ImportedPlaylist>,
    detail: Option<OnlinePlaylistDetail>,
    reference: String,
    resolved: Resource<String>,
    operations: Operations,
}

impl Management {
    pub(super) fn back(&mut self) -> bool {
        let open = self.detail.take().is_some();
        if open {
            self.operations.close_link();
        }
        open
    }

    pub(super) fn leave(&mut self) {
        self.identity = None;
        self.imports = Pages::default();
        self.clear_selection();
    }

    fn clear_selection(&mut self) {
        self.detail = None;
        self.reference.clear();
        self.resolved = Resource::default();
        self.operations.close_link();
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.;
            for (tab, label) in [
                (Tab::Local, "本地歌单"),
                (Tab::Account, "平台歌单"),
                (Tab::Imports, "已导入"),
            ] {
                let active = self.tab == tab;
                let label = tr(label);
                let r = ui.selectable_label(active, egui::RichText::new(label).size(15.));
                if active {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(r.rect.left() + 5., r.rect.bottom() - 3.),
                        egui::pos2(r.rect.right() - 5., r.rect.bottom()),
                    );
                    ui.painter().rect_filled(rect, 2, style::accent(ui));
                }
                if r.clicked() && !active {
                    self.tab = tab;
                    self.clear_selection();
                }
            }
        });
        ui.separator();
        ui.add_space(8.);
    }

    fn directory(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        if self.sources.status(ui, tr("正在读取音乐来源…")) {
            self.invalidate_sources();
            self.sync_sources(ui.ctx(), svc);
        }
    }

    fn imports(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        self.imports.poll();
        ui.horizontal(|ui| {
            ui.heading(tr("已导入的歌单"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, "refresh", tr("刷新"), 34., 18.).clicked() {
                    self.imports = Pages::default();
                }
            });
        });
        let mut load = !self.imports.started;
        egui::ScrollArea::vertical()
            .id_salt("imported-playlists")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.imports.items.is_empty()
                    && self.imports.next.is_none()
                    && self.imports.started
                    && !self.imports.busy()
                    && self.imports.error.is_none()
                {
                    ui.add_space(16.);
                    ui.weak(tr("还没有导入的歌单。可在平台歌单中选择歌单并导入。"));
                    if ui.button(tr("浏览平台歌单")).clicked() {
                        self.tab = Tab::Account;
                    }
                }
                for playlist in &self.imports.items {
                    let label = self
                        .sources
                        .value
                        .as_ref()
                        .and_then(|d| {
                            d.items
                                .iter()
                                .find(|s| s.instance_id == playlist.instance_id)
                        })
                        .map(|s| {
                            s.imported_label(crate::i18n::language_tags(), &playlist.account_id)
                        })
                        .unwrap_or_else(|| {
                            if playlist.account_id.is_empty() {
                                playlist.instance_id.clone()
                            } else {
                                format!("{} · {}", playlist.instance_id, playlist.account_id)
                            }
                        });
                    let title = if playlist.title.is_empty() {
                        &playlist.external_id
                    } else {
                        &playlist.title
                    };
                    ui.push_id(&playlist.playlist_id, |ui| {
                        egui::Frame::new()
                            .fill(style::surface(ui.visuals().dark_mode))
                            .corner_radius(8)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.strong(title);
                                        let status = tr(if playlist.stale {
                                            "待同步"
                                        } else {
                                            "已有快照"
                                        });
                                        ui.weak(format!("{label} · {status}"));
                                        if let Some(time) = &playlist.last_success_at {
                                            ui.small(tr("上次同步 {0}").replace("{0}", time));
                                        }
                                        if let Some(error) = &playlist.error {
                                            ui.colored_label(
                                                ui.visuals().error_fg_color,
                                                &error.code,
                                            );
                                        }
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_enabled(
                                                    !self.operations.busy(),
                                                    egui::Button::new(tr("立即同步")),
                                                )
                                                .clicked()
                                            {
                                                self.operations.sync(ui, svc, playlist.clone());
                                            }
                                            if let Some(job) = &playlist.job_id {
                                                if ui.button(tr("同步状态")).clicked() {
                                                    self.operations.watch(job.clone());
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(5.);
                    });
                }
                load |= more(ui, &self.imports);
            });
        if load {
            let server = svc.server.clone();
            let next = self.imports.next.clone();
            self.imports.load(ui.ctx(), svc.runtime, async move {
                server.imported_playlists(next.as_deref()).await
            });
        }
    }
}

fn more<T: Send + 'static>(ui: &mut egui::Ui, pages: &Pages<T>) -> bool {
    if pages.busy() {
        ui.spinner();
        return false;
    }
    if let Some(error) = &pages.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
        return ui.button(tr("重试")).clicked();
    }
    pages.can_load_more() && crate::ui::page_end_visible(ui)
}

impl SignedUi {
    pub(super) fn playlist_operations(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        if self.playlists.operations.status(ui, svc) {
            self.playlists.imports = Pages::default();
            self.lists = Pages::default();
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn playlists(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        self.playlists.tabs(ui);
        self.playlist_operations(ui, svc);
        match self.playlists.tab {
            Tab::Local => match self.local_playlists.list(ui, &self.lists) {
                Some(LocalAction::Open(id)) => self.navigate(Screen::Playlist(id)),
                Some(LocalAction::Refresh) => {
                    self.lists = Pages::default();
                    ui.ctx().request_repaint();
                }
                Some(LocalAction::More) => self.load_lists(ui.ctx(), svc),
                None => {}
            },
            Tab::Account => {
                self.playlists.directory(ui, svc);
                let outcome =
                    self.playlists
                        .account_page(ui, svc, &mut self.catalog.artwork, &self.lists);
                if outcome.more_local {
                    self.load_lists(ui.ctx(), svc);
                }
                if outcome.settings {
                    self.settings.open_sources();
                    self.navigate(Screen::Settings);
                }
                self.play_request(svc, outcome.play);
            }
            Tab::Imports => {
                self.playlists.directory(ui, svc);
                self.playlists.imports(ui, svc);
            }
        }
    }
}
