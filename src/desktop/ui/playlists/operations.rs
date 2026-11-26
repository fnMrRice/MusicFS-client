use crate::ui::style::ComboBoxMotion;
use crate::{
    browse::resource::Resource,
    i18n::tr,
    ui::{data::Pages, style, Services},
};
use eframe::egui;
use musicfs_server_adapter::{
    online_playlists::{ImportedPlaylist, OnlineSource, PlaylistLink, SyncJob},
    Playlist, ServerSession,
};
use std::time::{Duration, Instant};

#[derive(Clone)]
enum Command {
    Import(OnlineSource, String),
    Sync(ImportedPlaylist),
    Link {
        source: OnlineSource,
        external: String,
        target: String,
        remove: bool,
    },
}
impl Command {
    async fn run(self, server: ServerSession, key: String) -> Result<Option<String>, String> {
        match self {
            Self::Import(source, id) => server.import_playlist(&source, &id, &key).await.map(Some),
            Self::Sync(playlist) => server.sync_playlist(&playlist, &key).await.map(Some),
            Self::Link {
                source,
                external,
                target,
                remove,
            } => {
                server
                    .change_playlist_link(&source, &external, &target, remove)
                    .await?;
                Ok(None)
            }
        }
    }
}

struct LinkDialog {
    source: OnlineSource,
    external: String,
    writable: bool,
    target: String,
    links: Resource<Vec<PlaylistLink>>,
}
struct Watch {
    id: String,
    next_check: Option<Instant>,
    job: Resource<SyncJob>,
    terminal: bool,
}
#[derive(Default)]
pub(super) struct Operations {
    command: Option<(Command, String)>,
    result: Resource<Option<String>>,
    watch: Option<Watch>,
    link: Option<LinkDialog>,
    linked: bool,
}

impl Operations {
    pub(super) fn busy(&self) -> bool {
        self.result.busy()
    }
    pub(super) fn close_link(&mut self) {
        self.link = None;
    }
    pub(super) fn watch(&mut self, id: String) {
        self.watch = Some(Watch {
            id,
            next_check: None,
            job: Default::default(),
            terminal: false,
        });
    }
    pub(super) fn import(
        &mut self,
        ui: &egui::Ui,
        svc: Services<'_>,
        source: OnlineSource,
        external: String,
    ) {
        self.submit(ui.ctx(), svc, Command::Import(source, external));
    }
    pub(super) fn sync(&mut self, ui: &egui::Ui, svc: Services<'_>, playlist: ImportedPlaylist) {
        self.submit(ui.ctx(), svc, Command::Sync(playlist));
    }
    fn submit(&mut self, ctx: &egui::Context, svc: Services<'_>, command: Command) {
        if self.busy() {
            return;
        }
        self.command = Some((command, uuid::Uuid::new_v4().to_string()));
        self.linked = false;
        self.retry(ctx, svc);
    }
    fn retry(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        if let Some((action, key)) = self.command.clone() {
            self.result
                .start(ctx, svc.runtime, action.run(svc.server.clone(), key));
        }
    }
    pub(super) fn status(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> bool {
        let mut refresh = false;
        if self.result.poll() {
            if let Some(receipt) = self.result.value.take() {
                self.command = None;
                if let Some(job) = receipt {
                    self.watch(job);
                } else {
                    self.link = None;
                    self.linked = true;
                }
                refresh = true;
            }
        }
        if self.result.status(ui, tr("正在提交…")) {
            self.retry(ui.ctx(), svc);
        }
        if self.linked {
            ui.weak(tr("歌单关联已更新。"));
        }
        let Some(watch) = &mut self.watch else {
            return refresh;
        };
        if watch.job.poll() {
            let job = watch.job.value.as_ref().unwrap();
            watch.terminal = !matches!(job.state.as_str(), "queued" | "running");
            refresh |= watch.terminal;
            watch.next_check = Some(Instant::now() + Duration::from_secs(2));
        }
        let retry = if let Some(error) = &watch.job.error {
            ui.horizontal(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, error);
                style::button(ui, tr("重试")).clicked()
            })
            .inner
        } else {
            false
        };
        if let Some(job) = &watch.job.value {
            let state = match job.state.as_str() {
                "queued" => "排队中",
                "running" => "同步中",
                "succeeded" => "已完成",
                "failed" => "失败",
                "cancelled" => "已取消",
                s => s,
            };
            ui.horizontal(|ui| {
                ui.label(tr("歌单同步 · {0}").replace("{0}", tr(state)));
                if let Some(err) = &job.error {
                    ui.colored_label(ui.visuals().error_fg_color, &err.code);
                }
            });
        }
        if !watch.terminal && (retry || watch.job.error.is_none()) && !watch.job.busy() {
            let remaining = watch
                .next_check
                .map(|t| t.saturating_duration_since(Instant::now()))
                .unwrap_or_default();
            if retry || remaining.is_zero() {
                let server = svc.server.clone();
                let id = watch.id.clone();
                watch.job.start(ui.ctx(), svc.runtime, async move {
                    server.playlist_job(&id).await
                });
            } else {
                ui.ctx().request_repaint_after(remaining);
            }
        }
        refresh
    }

    pub(super) fn open_link(&mut self, source: OnlineSource, external: String, writable: bool) {
        self.link = Some(LinkDialog {
            source,
            external,
            writable,
            target: "favorites".into(),
            links: Default::default(),
        });
    }
    pub(super) fn link_dialog(
        &mut self,
        ctx: &egui::Context,
        svc: Services<'_>,
        local: &Pages<Playlist>,
    ) -> bool {
        let busy = self.busy();
        let Some(dialog) = &mut self.link else {
            return false;
        };
        let mut more = false;
        let mut close = false;
        let mut command = None;
        let modal = egui::Modal::new(egui::Id::new("platform-playlist-link")).show(ctx, |ui| {
            ui.set_width(380.);
            ui.heading(tr("本地歌单关联"));
            ui.add_space(8.);
            dialog.links.poll();
            if !dialog.links.started || dialog.links.status(ui, tr("正在读取关联…")) {
                let server = svc.server.clone();
                dialog.links.start(
                    ctx,
                    svc.runtime,
                    async move { server.playlist_links().await },
                );
            }
            if let Some(links) = &dialog.links.value {
                let linked = links.iter().find(|l| {
                    l.instance_id == dialog.source.instance_id
                        && l.external_id == dialog.external
                        && Some(l.account_id.as_str()) == dialog.source.account_id.as_deref()
                });
                let name = |id: &str| {
                    local
                        .items
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| {
                            if id == "favorites" {
                                tr("我的收藏").to_owned()
                            } else {
                                id.to_owned()
                            }
                        })
                };
                if let Some(link) = linked {
                    ui.label(tr("已关联 {0}").replace("{0}", &name(&link.playlist_id)));
                    if link.pending_count > 0 {
                        ui.weak(
                            tr("待同步 {0} 项").replace("{0}", &link.pending_count.to_string()),
                        );
                    }
                    if let Some(error) = &link.last_error_code {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if style::add_button(ui, !busy, egui::Button::new(tr("解除关联"))).clicked()
                    {
                        command = Some((link.playlist_id.clone(), true));
                    }
                } else if dialog.writable {
                    ui.label(tr("选择要关联的本地歌单"));
                    egui::ComboBox::from_id_salt("link-target")
                        .width(350.)
                        .selected_text(name(&dialog.target))
                        .show_animated(ui, |ui| {
                            style::selectable_value(
                                ui,
                                &mut dialog.target,
                                "favorites".into(),
                                tr("我的收藏"),
                            );
                            for p in &local.items {
                                style::selectable_value(
                                    ui,
                                    &mut dialog.target,
                                    p.id.clone(),
                                    &p.name,
                                );
                            }
                            if local.can_load_more() {
                                more = crate::ui::page_end_visible(ui);
                            }
                        });
                    if local.busy() {
                        ui.spinner();
                    }
                    if let Some(error) = &local.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                        more = style::button(ui, tr("重试读取歌单")).clicked();
                    }
                    if style::primary(ui, tr("确认关联"), !busy).clicked() {
                        command = Some((dialog.target.clone(), false));
                    }
                } else {
                    ui.weak(tr("此平台歌单只读。"));
                }
            }
            if let Some(error) = &self.result.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.);
            close = style::button(ui, tr("关闭")).clicked();
        });
        if let Some((target, remove)) = command {
            let command = Command::Link {
                source: dialog.source.clone(),
                external: dialog.external.clone(),
                target,
                remove,
            };
            self.submit(ctx, svc, command);
        } else if close || modal.should_close() {
            self.link = None;
        }
        more
    }
}
