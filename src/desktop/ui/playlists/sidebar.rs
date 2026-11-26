use super::{more, Management};
use crate::{
    i18n::{language_tags, tr},
    ui::{
        data::Pages,
        navigation::{entry, Screen, SourcePlaylist},
        Services,
    },
};
use eframe::egui;
use musicfs_server_adapter::online_playlists::{OnlinePlaylist, OnlineSource};

impl Management {
    pub(in crate::ui) fn invalidate_sources(&mut self) {
        self.sources = Default::default();
        self.accounts.clear();
        self.refresh_source = None;
        self.clear_selection();
    }

    pub(in crate::ui) fn sync_sources(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let revision = svc.activity.revisions().capabilities;
        if self.revision.replace(revision) != Some(revision) {
            self.invalidate_sources();
        }
        if !self.sources.started {
            let server = svc.server.clone();
            self.sources.start(
                ctx,
                svc.runtime,
                async move { server.playlist_sources().await },
            );
        }
        // 侧栏与管理页共用每个来源的请求和游标。账号变化后丢弃旧请求的接收端。
        if self.sources.poll() {
            let force = self.refresh_source.take();
            for source in &self.sources.value.as_ref().unwrap().items {
                if source.available("playlist.list") {
                    let page = self.accounts.entry(source.instance_id.clone()).or_default();
                    Self::load_account(
                        ctx,
                        svc,
                        source,
                        page,
                        force.as_ref() == Some(&source.instance_id),
                    );
                }
            }
        }
        for page in self.accounts.values_mut() {
            page.poll();
        }
    }

    pub(super) fn load_account(
        ctx: &egui::Context,
        svc: Services<'_>,
        source: &OnlineSource,
        page: &mut Pages<OnlinePlaylist>,
        refresh: bool,
    ) {
        let server = svc.server.clone();
        let source = source.clone();
        let next = page.next.clone();
        page.load(ctx, svc.runtime, async move {
            server
                .account_playlists(&source, next.as_deref(), refresh)
                .await
        });
    }

    pub(super) fn source(&self, key: &SourcePlaylist) -> Option<&OnlineSource> {
        self.sources.value.as_ref()?.items.iter().find(|source| {
            source.instance_id == key.instance
                && source.account_id == key.account
                && source.available("playlist.list")
        })
    }

    pub(super) fn known(&self, key: &SourcePlaylist) -> Option<&OnlinePlaylist> {
        self.accounts
            .get(&key.instance)?
            .items
            .iter()
            .find(|p| p.external_id == key.external)
    }

    pub(in crate::ui) fn source_title(&self, key: &SourcePlaylist) -> Option<String> {
        self.source(key)?;
        self.known(key)
            .or_else(|| {
                self.detail
                    .as_ref()
                    .filter(|p| p.key() == (&key.instance, &key.external))?
                    .playlist()
            })
            .map(|p| p.title.clone())
    }

    pub(in crate::ui) fn sidebar(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        active: &Screen,
        go: &mut Option<Screen>,
    ) -> bool {
        let Some(directory) = &self.sources.value else {
            self.directory(ui, svc);
            return false;
        };
        let mut empty = true;
        for source in &directory.items {
            let Some(page) = self.accounts.get_mut(&source.instance_id) else {
                continue;
            };
            if page.items.is_empty() && !page.busy() && page.error.is_none() && page.next.is_none()
            {
                continue;
            }
            empty = false;
            let expanded = egui::CollapsingHeader::new(source.label(language_tags()))
                .id_salt(("account-playlists-sidebar", &source.instance_id, &source.account_id))
                .default_open(true)
                .show(ui, |ui| {
                    let row = |ui: &mut egui::Ui, item: &OnlinePlaylist| {
                        let selected = matches!(active, Screen::SourcePlaylist(key)
                            if key.instance == source.instance_id && key.account == source.account_id && key.external == item.external_id);
                        entry(ui, &item.title, "disc", selected, true)
                    };
                    for item in page.items.iter().filter(|p| source.platform != "netease" || p.writable) {
                        if row(ui, item) {
                            *go = Some(playlist_screen(source, item));
                        }
                    }
                    if source.platform == "netease" && page.items.iter().any(|p| !p.writable) {
                        egui::CollapsingHeader::new(tr("收藏的歌单"))
                            .id_salt("saved-playlists")
                            .show(ui, |ui| {
                                for item in page.items.iter().filter(|p| !p.writable) {
                                    if row(ui, item) {
                                        *go = Some(playlist_screen(source, item));
                                    }
                                }
                            });
                    }
                    more(ui, page)
                });
            if expanded.body_returned == Some(true) {
                Self::load_account(ui.ctx(), svc, source, page, false);
            }
        }
        empty
    }
}

fn playlist_screen(source: &OnlineSource, item: &OnlinePlaylist) -> Screen {
    Screen::SourcePlaylist(SourcePlaylist {
        instance: source.instance_id.clone(),
        account: source.account_id.clone(),
        external: item.external_id.clone(),
    })
}
