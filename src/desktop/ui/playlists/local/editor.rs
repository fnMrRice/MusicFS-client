use super::rules::Rules;
use crate::{
    i18n::tr,
    icons,
    task::Task,
    ui::{
        data::{Pages, Services},
        style,
    },
};
use eframe::egui::{self, Align, Context, Frame, RichText};
use musicfs_server_adapter::Playlist;
use serde_json::Value;
use uuid::Uuid;

pub(super) enum Outcome {
    Stay,
    Close,
    Saved,
}

pub(super) struct Editor {
    id: Option<String>,
    revision: Option<u64>,
    name: String,
    bindings: Vec<Value>,
    rules: Rules,
    loading: Option<Task<(Playlist, Vec<Value>)>>,
    saving: Option<Task<()>>,
    key: Option<String>,
    error: Option<String>,
    conflict: bool,
    dirty: bool,
    focus_name: bool,
}

impl Editor {
    pub(super) fn new(id: Option<String>) -> Self {
        Self {
            id,
            revision: None,
            name: String::new(),
            bindings: vec![],
            rules: Rules::default(),
            loading: None,
            saving: None,
            key: None,
            error: None,
            conflict: false,
            dirty: false,
            focus_name: true,
        }
    }

    fn load(&mut self, ctx: &Context, svc: Services<'_>) {
        let Some(id) = self.id.clone() else {
            return;
        };
        let server = svc.server.clone();
        self.error = None;
        self.conflict = false;
        self.revision = None;
        self.loading = Some(Task::spawn(svc.runtime, ctx, async move {
            server.playlist_rules(&id).await
        }));
    }

    fn save(&mut self, ctx: &Context, svc: Services<'_>) {
        let server = svc.server.clone();
        let key = self
            .key
            .get_or_insert_with(|| Uuid::new_v4().to_string())
            .clone();
        let target = self.id.clone().zip(self.revision);
        let name = self.name.trim().to_owned();
        let bindings = self.bindings.clone();
        self.error = None;
        self.saving = Some(Task::spawn(svc.runtime, ctx, async move {
            server
                .save_playlist_rules(
                    target.as_ref().map(|(id, rev)| (id.as_str(), *rev)),
                    &name,
                    &bindings,
                    &key,
                )
                .await
        }));
    }

    pub(super) fn show(
        &mut self,
        ctx: &Context,
        svc: Services<'_>,
        lists: &Pages<Playlist>,
    ) -> Outcome {
        if let Some(result) = self.loading.as_mut().and_then(Task::poll) {
            self.loading = None;
            match result {
                Ok((playlist, bindings)) => {
                    self.name = playlist.name;
                    self.revision = Some(playlist.rule_revision);
                    self.bindings = bindings;
                    self.rules = Rules::default();
                    self.dirty = false;
                    self.focus_name = true;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(result) = self.saving.as_mut().and_then(Task::poll) {
            self.saving = None;
            match result {
                Ok(()) => return Outcome::Saved,
                Err(error) => {
                    self.conflict = error.contains("HTTP 409") || error.contains("HTTP 412");
                    // 只有明确拒绝的请求才允许改写，网络中断后重试同一请求。
                    if error.contains("HTTP 4") {
                        self.key = None;
                    }
                    self.error = Some(error);
                }
            }
        }
        let editing = self.id.is_some();
        let ready = !editing || self.revision.is_some();
        if !ready && self.loading.is_none() && self.error.is_none() {
            self.load(ctx, svc);
        }
        let busy = self.saving.is_some();
        let mutable = ready && !busy && self.key.is_none() && !self.conflict;
        let title = if editing {
            tr("编辑歌单")
        } else {
            tr("新建歌单")
        };
        let mut outcome = Outcome::Stay;
        let mut submit = false;
        let mut reload = false;
        let modal = egui::Modal::new(egui::Id::new("local-playlist-editor"))
            .frame(Frame::window(&ctx.style()).inner_margin(22))
            .show(ctx, |ui| {
                ui.set_width(500.);
                ui.horizontal(|ui| {
                    let (icon, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(36.), egui::Sense::hover());
                    ui.painter().rect_filled(icon, 8, style::accent_soft(ui));
                    icons::paint(ui, "queue", icon.shrink(8.), style::accent(ui));
                    ui.label(RichText::new(title).font(style::bold(ui, 21.)));
                });
                ui.add_space(5.);
                ui.separator();
                if self.loading.is_some() {
                    ui.spinner();
                }
                if ready {
                    ui.add_enabled_ui(mutable, |ui| {
                        ui.label(RichText::new(tr("歌单名称")).font(style::bold(ui, 14.)));
                        let name = ui.add_sized(
                            [ui.available_width(), 36.],
                            egui::TextEdit::singleline(&mut self.name)
                                .margin(egui::Margin::symmetric(9, 8))
                                .hint_text(tr("输入歌单名称"))
                                .char_limit(256),
                        );
                        if self.focus_name {
                            name.request_focus();
                            self.focus_name = false;
                        }
                        self.dirty |= name.changed();
                        submit = name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.add_space(5.);
                        ui.label(RichText::new(tr("聚合来源")).font(style::bold(ui, 14.)));
                        let max_height = (ctx.screen_rect().height() - 315.).clamp(120., 450.);
                        egui::ScrollArea::vertical()
                            .id_salt("playlist-rules")
                            .auto_shrink([false, true])
                            .max_height(max_height)
                            .show(ui, |ui| {
                                self.dirty |= self.rules.show(
                                    ui,
                                    svc,
                                    lists,
                                    self.id.as_deref(),
                                    &mut self.bindings,
                                );
                            });
                    });
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if self.conflict {
                        ui.weak(tr("歌单已在其他位置更改，重新载入后可继续编辑。"));
                    }
                    if editing && (!ready || self.conflict) && self.loading.is_none() {
                        reload = style::button(ui, tr("重新载入歌单")).clicked();
                    }
                }
                ui.add_space(5.);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let retry = self.key.is_some() && !busy;
                    let label = if busy {
                        tr("正在保存…")
                    } else if retry {
                        tr("重试上次操作")
                    } else if editing {
                        tr("保存修改")
                    } else {
                        tr("创建歌单")
                    };
                    let valid = (mutable
                        && !self.name.trim().is_empty()
                        && !self.name.chars().any(char::is_control)
                        && self.dirty)
                        || retry;
                    submit = style::primary(ui, label, valid).clicked() || (valid && submit);
                    if style::add_button(ui, !busy, egui::Button::new(tr("取消"))).clicked() {
                        outcome = Outcome::Close;
                    }
                });
            });
        if reload {
            self.load(ctx, svc);
        }
        if submit {
            self.save(ctx, svc);
        }
        if !busy && !submit && modal.should_close() {
            return Outcome::Close;
        }
        outcome
    }
}
