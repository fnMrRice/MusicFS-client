mod tasks;

use super::{data::Services, navigation::Screen, style, themes, HostAction, SignedUi};
use crate::{i18n::tr, icons, model::Preferences, task::Task};
use eframe::egui::{self, Align, Color32, Layout, RichText, Ui, UiBuilder};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Shell {
    pub(super) history: Vec<Screen>,
    pub(super) navigation: Option<bool>,
    health: Option<Task<bool>>,
    checked: Option<Instant>,
    result: Option<Result<bool, String>>,
    tasks: tasks::Queue,
}

fn popup(ui: &Ui, key: &str, button: &egui::Response, width: f32, contents: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(key);
    if button.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup_below_widget(
        ui,
        id,
        button,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                contents(ui);
            });
        },
    );
}

impl Shell {
    fn check(&mut self, ui: &Ui, svc: Services<'_>) {
        let server = svc.server.clone();
        self.checked = Some(Instant::now());
        self.health = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            let ready = server
                .settings_json("GET", "/health/ready", None, &[])
                .await?;
            if ready["instanceId"].as_str() != Some(server.info.instance_id.as_str()) {
                return Err(tr("服务已变更").into());
            }
            ready["ready"]
                .as_bool()
                .ok_or_else(|| tr("服务不兼容").into())
        }));
    }

    fn service_button(&mut self, ui: &mut Ui, svc: Services<'_>) {
        if let Some(result) = self.health.as_mut().and_then(Task::poll) {
            self.health = None;
            self.result = Some(result);
        }
        if self.health.is_none()
            && self
                .checked
                .is_none_or(|i| i.elapsed() > Duration::from_secs(60))
        {
            self.check(ui, svc);
        }
        let (tone, status) = match &self.result {
            Some(Ok(true)) => (Color32::from_rgb(116, 182, 155), tr("运行正常")),
            Some(Ok(false)) => (Color32::from_rgb(222, 173, 96), tr("服务未就绪")),
            Some(Err(_)) => (ui.visuals().error_fg_color, tr("服务异常")),
            None => (ui.visuals().weak_text_color(), tr("正在检查…")),
        };
        let response = icons::button(ui, "server", tr("服务状态"), 36., 22.);
        let dot = response.rect.center() + egui::vec2(10., 10.);
        ui.painter().circle(
            dot,
            4.,
            tone,
            egui::Stroke::new(1.5_f32, style::background(ui.visuals().dark_mode)),
        );
        popup(ui, "server-status", &response, 250., |ui| {
            ui.strong(tr("服务状态"));
            ui.colored_label(tone, status);
            ui.label(&svc.server.info.endpoint);
            ui.small(format!("MusicFS {}", svc.server.info.server_version));
            if let Some(Err(message)) = &self.result {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            if ui
                .add_enabled(self.health.is_none(), egui::Button::new(tr("重新检查")))
                .clicked()
            {
                self.check(ui, svc);
            }
        });
    }
}

impl SignedUi {
    pub(super) fn header(
        &mut self,
        ui: &mut Ui,
        svc: Services<'_>,
        prefs: &Preferences,
        action: &mut Option<HostAction>,
    ) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 88.), egui::Sense::hover());
        let gap = if rect.width() >= 650. { 14. } else { 5. };
        let split = rect.right() - 5. * 36. - 4. * gap;
        let mut left = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.with_max_x(split - 8.))
                .layout(Layout::left_to_right(Align::Center)),
        );
        let wide = ui.ctx().screen_rect().width() > 850.;
        left.spacing_mut().item_spacing.x = if wide { 18. } else { 8. };
        if icons::button(&mut left, "back", tr("返回"), 36., 22.).clicked() {
            self.back(wide);
        }
        if !wide && icons::button(&mut left, "grid", tr("主导航"), 36., 20.).clicked() {
            self.shell.navigation = Some(!self.shell.navigation.unwrap_or(false));
        }
        left.add(
            egui::Label::new(
                RichText::new(self.title())
                    .font(style::bold(&left, 23.))
                    .strong(),
            )
            .truncate(),
        );
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.with_min_x(split))
                .layout(Layout::left_to_right(Align::Center)),
        );
        right.spacing_mut().item_spacing.x = gap;
        self.shell.service_button(&mut right, svc);
        if self.shell.tasks.button(&mut right, svc) {
            self.catalog.categories = Default::default();
            self.catalog.artwork = Default::default();
            self.catalog.reset_tracks();
        }
        if icons::button(&mut right, "settings", tr("设置"), 36., 22.).clicked() {
            self.navigate(Screen::Settings);
        }
        let theme = icons::button(&mut right, "theme", tr("换肤"), 36., 22.);
        popup(&right, "theme-menu", &theme, 160., |ui| {
            for (key, label) in themes() {
                if ui.selectable_label(prefs.theme == key, tr(label)).clicked() {
                    *action = Some(HostAction::Theme(key.into()));
                    ui.memory_mut(|m| m.close_popup());
                }
            }
            ui.separator();
            if let Some(value) = style::accent_choices(ui, &prefs.accent) {
                *action = Some(HostAction::Accent(value));
            }
        });
        let (r, avatar) = right.allocate_exact_size(egui::Vec2::splat(36.), egui::Sense::click());
        let dark = right.visuals().dark_mode;
        let ink = if avatar.hovered() {
            style::accent(&right)
        } else {
            style::text(dark)
        };
        right.painter().circle(
            r.center(),
            18.,
            style::hovered(dark),
            egui::Stroke::new(1.0_f32, style::line(dark)),
        );
        icons::paint(
            &right,
            "user",
            egui::Rect::from_center_size(r.center(), egui::Vec2::splat(21.)),
            ink,
        );
        avatar.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr("账号菜单"))
        });
        let avatar = avatar.on_hover_text(tr("账号菜单"));
        popup(&right, "account-menu", &avatar, 240., |ui| {
            ui.strong(&svc.server.info.user_name);
            ui.weak(&svc.server.info.endpoint);
            ui.separator();
            if ui.button(tr("账号与设置")).clicked() {
                self.navigate(Screen::Settings);
                ui.memory_mut(|m| m.close_popup());
            }
            if ui.button(tr("退出登录")).clicked() {
                *action = Some(HostAction::Logout);
                ui.memory_mut(|m| m.close_popup());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_restores_routes_then_toggles_the_navigation_at_the_root() {
        let mut app = SignedUi::default();
        app.navigate(Screen::Search);
        app.navigate(Screen::Settings);
        app.back(true);
        assert!(app.screen == Screen::Search);
        app.back(true);
        assert!(app.screen == Screen::Library);
        app.back(true);
        assert_eq!(app.shell.navigation, Some(false));
        app.back(true);
        assert_eq!(app.shell.navigation, Some(true));
        app.navigate(Screen::History);
        assert!(app.shell.navigation.is_none());
        app.back(false);
        app.back(false);
        assert_eq!(app.shell.navigation, Some(true));
    }
}
