use super::{more, Management};
use crate::{
    browse::{playlist_cards, OnlinePlaylistDetail, QueueSelection},
    i18n::{language_tags, tr},
    icons,
    ui::{data::Pages, navigation::SourcePlaylist, style, Artwork, Services},
};
use eframe::egui;
use musicfs_server_adapter::{online_playlists::OnlineSource, Playlist};

#[derive(Default)]
pub(in crate::ui) struct Outcome {
    pub play: Option<QueueSelection>,
    pub settings: bool,
    pub more_local: bool,
}

impl Management {
    pub(super) fn account_page(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        art: &mut Artwork,
        local: &Pages<Playlist>,
    ) -> Outcome {
        let mut outcome = Outcome::default();
        let Some(directory) = &self.sources.value else {
            return outcome;
        };
        let sources: Vec<_> = directory
            .items
            .iter()
            .filter(|s| s.is_music_source())
            .collect();
        if sources.is_empty() {
            ui.weak(tr("服务器没有可用的平台歌单来源。"));
            outcome.settings = ui.button(tr("音乐源设置")).clicked();
            return outcome;
        }
        if !sources.iter().any(|s| s.instance_id == self.selected) {
            self.selected = sources
                .iter()
                .find(|s| s.available("playlist.list"))
                .unwrap_or(&sources[0])
                .instance_id
                .clone();
        }
        let current = (*sources
            .iter()
            .copied()
            .find(|s| s.instance_id == self.selected)
            .unwrap())
        .clone();
        let previous = self.selected.clone();
        let mut refresh = false;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("playlist-source")
                .width(240.)
                .selected_text(current.label(language_tags()))
                .show_ui(ui, |ui| {
                    for source in sources {
                        ui.selectable_value(
                            &mut self.selected,
                            source.instance_id.clone(),
                            source.label(language_tags()),
                        );
                    }
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                refresh = icons::button(ui, "refresh", tr("刷新"), 34., 18.).clicked();
            });
        });
        ui.add_space(8.);
        if refresh {
            self.invalidate_sources();
            self.refresh_source = Some(current.instance_id.clone());
            ui.ctx().request_repaint();
            return outcome;
        }
        let identity = (
            current.instance_id.clone(),
            current.account_id.clone(),
            current.account_revision.clone(),
        );
        if self.identity.as_ref() != Some(&identity) {
            self.identity = Some(identity);
            self.clear_selection();
        }
        if previous != self.selected {
            self.clear_selection();
            ui.ctx().request_repaint();
            return outcome;
        }
        if current.availability != "available" {
            ui.weak(tr("请检查音乐源设置中的平台账号与连接状态。"));
            outcome.settings = ui.button(tr("音乐源设置")).clicked();
            return outcome;
        }
        art.poll(ui.ctx());
        if self.detail.is_some() {
            if ui.button(tr("返回平台歌单")).clicked() {
                self.back();
                return outcome;
            }
            return self.detail_page(ui, svc, art, local, &current);
        }
        if current.available("playlist.observe") {
            self.resolved.poll();
            if let Some(id) = self.resolved.value.take() {
                self.detail = Some(OnlinePlaylistDetail::new(
                    &current.instance_id,
                    &id,
                    None,
                    current.available("stream.resolve"),
                ));
                self.resolved = Default::default();
                ui.ctx().request_repaint();
                return outcome;
            }
            let mut submit = false;
            ui.horizontal(|ui| {
                let hint = tr(if current.platform == "bilibili" {
                    "收藏夹链接或编号"
                } else {
                    "输入平台歌单 ID"
                });
                let r = ui.add_sized(
                    [260., 32.],
                    egui::TextEdit::singleline(&mut self.reference).hint_text(hint),
                );
                submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                submit |= ui
                    .add_enabled(
                        !self.reference.trim().is_empty() && !self.resolved.busy(),
                        egui::Button::new(tr("预览歌单")),
                    )
                    .clicked();
            });
            let retry = self.resolved.status(ui, tr("正在读取在线歌单…"));
            if (submit || retry) && !self.resolved.busy() && !self.reference.trim().is_empty() {
                let source = current.clone();
                let reference = self.reference.trim().to_owned();
                let server = svc.server.clone();
                self.resolved.start(ui.ctx(), svc.runtime, async move {
                    server.playlist_reference(&source, &reference).await
                });
            }
            ui.add_space(10.);
        }
        if current.available("playlist.list") {
            let account = self.accounts.get_mut(&current.instance_id).unwrap();
            let mut load = false;
            egui::ScrollArea::vertical()
                .id_salt(("account-playlists", &current.instance_id))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let picked = playlist_cards(
                        ui,
                        account.items.len(),
                        |i| (&account.items[i], ""),
                        |ui, r, c| art.paint(ui, r, svc, c),
                    );
                    if let Some(i) = picked {
                        let p = &account.items[i];
                        self.detail = Some(OnlinePlaylistDetail::new(
                            &current.instance_id,
                            &p.external_id,
                            p.cover_url.clone(),
                            current.available("stream.resolve"),
                        ));
                        ui.ctx().request_repaint();
                    }
                    if account.items.is_empty()
                        && account.next.is_none()
                        && account.started
                        && account.error.is_none()
                        && !account.busy()
                    {
                        ui.weak(tr("此账号暂无平台歌单。"));
                    }
                    load |= more(ui, account);
                });
            if load {
                Self::load_account(ui.ctx(), svc, &current, account, false);
            }
        } else if !current.available("playlist.observe") {
            ui.weak(tr("此来源暂不提供歌单浏览。"));
            outcome.settings = ui.button(tr("音乐源设置")).clicked();
        }
        outcome
    }

    pub(in crate::ui) fn source_detail(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        art: &mut Artwork,
        local: &Pages<Playlist>,
        key: &SourcePlaylist,
    ) -> Outcome {
        self.directory(ui, svc);
        if self.sources.value.is_none() {
            return Outcome::default();
        }
        let Some(source) = self.source(key).cloned() else {
            ui.weak(tr("此歌单已删除或无法访问。"));
            return Outcome::default();
        };
        if !self
            .detail
            .as_ref()
            .is_some_and(|p| p.key() == (&key.instance, &key.external))
        {
            self.clear_selection();
            self.detail = Some(OnlinePlaylistDetail::new(
                &key.instance,
                &key.external,
                self.known(key).and_then(|p| p.cover_url.clone()),
                source.available("stream.resolve"),
            ));
        }
        art.poll(ui.ctx());
        self.detail_page(ui, svc, art, local, &source)
    }

    fn detail_page(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        art: &mut Artwork,
        local: &Pages<Playlist>,
        current: &OnlineSource,
    ) -> Outcome {
        let mut outcome = Outcome::default();
        let detail = self.detail.as_mut().unwrap();
        let external = detail.key().1.to_owned();
        let known = self
            .accounts
            .get(&current.instance_id)
            .and_then(|page| page.items.iter().find(|p| p.external_id == external));
        let writable =
            known.is_some_and(|p| p.writable) || detail.playlist().is_some_and(|p| p.writable);
        ui.horizontal(|ui| {
            if current.available("playlist.observe")
                && current.import_account().is_some()
                && style::primary(
                    ui,
                    tr("导入歌单"),
                    !self.operations.busy() && detail.playlist().is_some(),
                )
                .clicked()
            {
                self.operations
                    .import(ui, svc, current.clone(), external.clone());
            }
            if current.available("playlist.addTracks")
                && current.account_id.is_some()
                && detail.playlist().is_some()
                && ui.button(tr("本地歌单关联")).clicked()
            {
                self.operations
                    .open_link(current.clone(), external.clone(), writable);
            }
        });
        outcome.more_local = self.operations.link_dialog(ui.ctx(), svc, local);
        ui.add_space(8.);
        outcome.play = egui::ScrollArea::vertical()
            .id_salt(("playlist-tracks", &external))
            .auto_shrink([false, false])
            .show(ui, |ui| detail.show(ui, svc, art))
            .inner;
        outcome
    }
}
