use super::style;
use crate::i18n::tr;
use crate::model::DesktopState;
use eframe::egui::{self, Align, Layout, RichText, Ui, Vec2};

mod field;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Login {
    pub endpoint: String,
    token: String,
    remember: bool,
    form_height: f32,
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
                let content = if crate::system::has_custom_titlebar(ctx) {
                    super::shell::caption::login_title_bar(ui)
                } else {
                    r
                };
                let right = content.with_min_x(r.left() + artwork);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(right)
                        .layout(Layout::top_down(Align::Center)),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let inset = ((right.height() - self.form_height) * 0.5).max(20.);
                                ui.add_space(if self.form_height > 0. { inset } else { 20. });
                                let width = (ui.available_width() - 48.).clamp(1., 380.);
                                let form = ui.allocate_ui_with_layout(
                                    Vec2::new(width, 0.),
                                    Layout::top_down(Align::Min),
                                    |ui| self.form(ui, state),
                                );
                                ui.add_space(20.);
                                let height = form.response.rect.height();
                                if (height - self.form_height).abs() > 0.5 {
                                    self.form_height = height;
                                    ctx.request_discard("Center login form");
                                }
                            });
                    },
                );
            });
    }

    fn form(&mut self, ui: &mut Ui, state: &mut DesktopState) {
        let dark = ui.visuals().dark_mode;
        style::note(ui);
        ui.add_space(18.);
        ui.heading(tr("连接音乐库"));
        ui.label(
            RichText::new(tr("使用服务器地址和管理员提供的令牌登录 MusicFS。"))
                .color(style::muted(dark)),
        );
        ui.add_space(12.);
        field::Field::Endpoint.show(ui, &mut self.endpoint, !state.busy);
        ui.add_space(6.);
        let password = field::Field::Token.show(ui, &mut self.token, !state.busy);
        if let Some(error) = &state.error {
            let ink = if dark {
                egui::Color32::from_rgb(248, 159, 166)
            } else {
                egui::Color32::from_rgb(151, 49, 64)
            };
            egui::Frame::NONE
                .fill(style::background(dark).lerp_to_gamma(ink, 0.09))
                .corner_radius(7)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(RichText::new(error).size(13.).color(ink));
                });
        }
        ui.add_enabled_ui(!state.busy, |ui| {
            style::checkbox(ui, &mut self.remember, tr("在此设备上保持登录"));
        });
        let enabled =
            !state.busy && !self.endpoint.trim().is_empty() && !self.token.trim().is_empty();
        let enter = password.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let label = tr(if state.busy {
            "正在连接…"
        } else {
            "连接并登录"
        });
        let submit = ui.vertical_centered(|ui| {
            style::primary_sized(ui, label, enabled, Vec2::new(ui.available_width(), 44.))
        });
        if submit.inner.clicked() || (enabled && enter) {
            let token = std::mem::take(&mut self.token);
            state.connect(
                self.endpoint.trim().to_owned(),
                token,
                self.remember,
                ui.ctx(),
            );
            self.remember = false;
        }
        if !state.busy && state.prefs.saved_login.is_some() {
            ui.horizontal_wrapped(|ui| {
                if style::button(ui, tr("重试已保存的登录")).clicked() {
                    state.restore(ui.ctx());
                }
                if style::button(ui, tr("忘记已保存的登录")).clicked() {
                    state.forget();
                }
            });
        }
        ui.add_space(8.);
        if let Some(language) = super::settings::language_menu(ui, &state.prefs.language) {
            state.prefs.language = language;
            state.save_preferences();
            ui.ctx().request_repaint();
        }
    }
}
