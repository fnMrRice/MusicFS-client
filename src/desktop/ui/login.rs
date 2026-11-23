use super::style;
use crate::i18n::tr;
use crate::model::DesktopState;
use eframe::egui::{self, Align, Color32, Layout, RichText, Vec2};

#[derive(Default)]
pub struct Login {
    pub endpoint: String,
    token: String,
}

impl Login {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut DesktopState) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(style::background(ctx.style().visuals.dark_mode)))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                let artwork = if r.width() >= 680.0 {
                    r.width() * 0.48
                } else {
                    0.0
                };
                if artwork > 0.0 {
                    style::vinyl(
                        ui,
                        egui::Rect::from_min_size(r.min, Vec2::new(artwork, r.height())),
                    );
                }
                let right = egui::Rect::from_min_max(r.min + Vec2::new(artwork, 0.0), r.max);
                let form_width = (right.width() - 50.0).clamp(180.0, 380.0);
                let form = egui::Rect::from_center_size(
                    right.center(),
                    Vec2::new(form_width, 435.0_f32.min(right.height() - 12.0)),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(form)
                        .layout(Layout::top_down_justified(Align::Min)),
                    |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.set_min_width((form_width - 9.0).max(160.0));
                            style::note(ui);
                            ui.add_space(27.0);
                            ui.heading(tr("连接音乐库"));
                            ui.weak(tr("使用服务器地址和管理员提供的令牌登录 MusicFS。"));
                            ui.add_space(16.0);
                            let endpoint_label = ui.label(tr("服务器地址"));
                            ui.add_enabled(
                                !state.busy,
                                egui::TextEdit::singleline(&mut self.endpoint)
                                    .hint_text("http://127.0.0.1:8080")
                                    .desired_width(f32::INFINITY),
                            )
                            .labelled_by(endpoint_label.id);
                            ui.add_space(8.0);
                            let token_label = ui.label(tr("管理员登录令牌"));
                            let password = ui
                                .add_enabled(
                                    !state.busy,
                                    egui::TextEdit::singleline(&mut self.token)
                                        .password(true)
                                        .hint_text(tr("输入登录令牌"))
                                        .desired_width(f32::INFINITY),
                                )
                                .labelled_by(token_label.id);
                            if let Some(error) = &state.error {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            }
                            ui.add_space(12.0);
                            let enabled = !state.busy
                                && !self.endpoint.trim().is_empty()
                                && !self.token.trim().is_empty();
                            let enter = password.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let button = egui::Button::new(
                                RichText::new(if state.busy {
                                    tr("正在连接…")
                                } else {
                                    tr("连接并登录")
                                })
                                .strong()
                                .color(Color32::WHITE),
                            )
                            .fill(style::ACCENT)
                            .corner_radius(7)
                            .min_size(Vec2::new(ui.available_width(), 40.0));
                            if ui.add_enabled(enabled, button).clicked() || (enabled && enter) {
                                let token = std::mem::take(&mut self.token);
                                state.connect(self.endpoint.trim().to_owned(), token, ctx);
                            }
                            ui.add_space(10.0);
                            if let Some(language) =
                                super::settings::language_menu(ui, &state.prefs.language)
                            {
                                state.prefs.language = language;
                                state.save_preferences();
                                ctx.request_repaint();
                            }
                        });
                    },
                );
            });
    }
}
