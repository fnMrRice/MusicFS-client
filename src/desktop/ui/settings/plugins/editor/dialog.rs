use super::super::catalog::localized;
use super::{Editor, Submission};
use crate::{i18n::tr, icons, ui::style};
use eframe::egui::{self, Align, Context, Frame, Layout, Margin, RichText, Ui};
use serde_json::json;

const PADDING: i8 = 22;
const FOOTER_PADDING: i8 = 12;
const BUTTON_HEIGHT: f32 = 36.;

pub(super) enum DialogAction {
    None,
    Close,
    Reload,
    Retry,
    RetryJob,
    Submit(Submission),
}

impl Editor {
    pub(super) fn dialog(&mut self, ctx: &Context) -> DialogAction {
        let busy = self.posting.is_some();
        let blocked = busy || self.loading.is_some() || self.conflict || self.pending.is_some();
        let screen = ctx.screen_rect().shrink(12.);
        let frame = Frame::window(&ctx.style()).inner_margin(0);
        let mut action = DialogAction::None;
        let modal = egui::Modal::new(egui::Id::new("plugin-settings-dialog"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width((screen.width() - frame.total_margin().sum().x).clamp(0., 544.));
                ui.spacing_mut().item_spacing.y = 0.;
                let header = Frame::new()
                    .inner_margin(Margin::symmetric(PADDING, 16))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.;
                            let (rect, _) = ui
                                .allocate_exact_size(egui::Vec2::splat(34.), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 9, style::accent_soft(ui));
                            icons::paint(ui, "settings", rect.shrink(8.), style::accent(ui));
                            let title = tr("配置 {0}").replace("{0}", &self.name);
                            ui.add(
                                egui::Label::new(RichText::new(title).font(style::bold(ui, 20.)))
                                    .truncate(),
                            );
                        });
                    });
                ui.add(egui::Separator::default().spacing(1.));
                let footer_height = BUTTON_HEIGHT + 2. * f32::from(FOOTER_PADDING);
                let body_height = screen.height()
                    - frame.total_margin().sum().y
                    - header.response.rect.height()
                    - footer_height
                    - 2.;
                egui::ScrollArea::vertical()
                    .id_salt((&self.plugin_id, "settings-form"))
                    .auto_shrink([false, true])
                    .max_height(body_height.max(1.))
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(Margin::symmetric(PADDING, 16))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 6.;
                                self.body(ui, busy, blocked, &mut action);
                            });
                    });
                ui.add(egui::Separator::default().spacing(1.));
                let radius = ui.visuals().window_corner_radius;
                Frame::new()
                    .fill(style::sidebar(ui.visuals().dark_mode))
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        ..radius
                    })
                    .inner_margin(Margin::symmetric(PADDING, FOOTER_PADDING))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), BUTTON_HEIGHT),
                            Layout::right_to_left(Align::Center),
                            |ui| self.footer(ui, busy, blocked, &mut action),
                        );
                    });
            });
        if matches!(action, DialogAction::None) && !busy && modal.should_close() {
            return DialogAction::Close;
        }
        action
    }

    fn body(&mut self, ui: &mut Ui, busy: bool, blocked: bool, action: &mut DialogAction) {
        if self.loading.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr("正在读取设置…"));
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.loading.is_none()
            && (self.data.is_none() || self.conflict || self.pending.is_some())
        {
            ui.horizontal_wrapped(|ui| {
                if self.pending.is_some() && !busy && ui.button(tr("重试上次操作")).clicked()
                {
                    *action = DialogAction::Retry;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new(tr("重新加载设置")))
                    .clicked()
                {
                    *action = DialogAction::Reload;
                }
            });
        }
        if let Some(data) = &self.data {
            let schema = &data.schema;
            let title = localized(&schema.title, &schema.localized_titles);
            ui.label(RichText::new(title).font(style::bold(ui, 16.)));
            let description = localized(&schema.description, &schema.localized_descriptions);
            if !description.is_empty() {
                ui.label(
                    RichText::new(description)
                        .size(13.)
                        .color(style::muted(ui.visuals().dark_mode)),
                );
            }
            ui.add_enabled_ui(!blocked, |ui| {
                for field in &schema.fields {
                    ui.add_space(8.);
                    ui.push_id(&field.key, |ui| field.show(ui, &mut self.draft));
                }
            });
            if !schema.actions.is_empty() {
                ui.add_space(8.);
                let dirty = self.draft != self.original;
                if dirty {
                    ui.weak(tr("请先保存修改，再执行插件操作。"));
                }
                let active = self.job.as_ref().is_some_and(|job| job.active());
                for item in &schema.actions {
                    if ui
                        .add_enabled(
                            !blocked && !dirty && !active,
                            egui::Button::new(item.text.label()),
                        )
                        .clicked()
                    {
                        *action = DialogAction::Submit(Submission::Action(item.id.clone()));
                    }
                    item.text.help(ui);
                }
            }
        }
        if let Some(message) = self.message {
            ui.label(tr(message));
        }
        if let Some(job) = &self.job {
            job.show(ui);
        }
        if let Some(error) = &self.job_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui
                .add_enabled(self.job_read.is_none(), egui::Button::new(tr("重试")))
                .clicked()
            {
                *action = DialogAction::RetryJob;
            }
        }
    }

    fn footer(&self, ui: &mut Ui, busy: bool, blocked: bool, action: &mut DialogAction) {
        if let Some(data) = self
            .data
            .as_ref()
            .filter(|data| !data.schema.fields.is_empty())
        {
            let values = (!blocked && self.draft != self.original)
                .then(|| data.schema.values(&self.draft))
                .flatten();
            let label = tr(if busy {
                "正在保存…"
            } else {
                "保存设置"
            });
            if style::primary(ui, label, values.is_some()).clicked() {
                *action = DialogAction::Submit(Submission::Save(json!({
                    "expectedRevision": data.revision, "schemaRevision": data.schema.revision, "values": values,
                })));
            }
        }
        let cancel = egui::Button::new(tr("取消")).min_size(egui::vec2(72., BUTTON_HEIGHT));
        if ui.add_enabled(!busy, cancel).clicked() {
            *action = DialogAction::Close;
        }
    }
}
