pub(super) mod caption;
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

fn popup_open(ui: &Ui, key: &str) -> bool {
    ui.memory(|m| m.is_popup_open(ui.make_persistent_id(key)))
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
        let open = popup_open(ui, "server-status");
        let response = icons::selectable_button(ui, "server", tr("服务状态"), 36., 22., open);
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
            if style::add_button(ui, self.health.is_none(), egui::Button::new(tr("重新检查")))
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
        let custom = crate::system::has_custom_titlebar(ui.ctx());
        let height = if custom { 64. } else { 88. };
        let (mut rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        if custom {
            rect.max.x = ui.ctx().screen_rect().right();
        }
        let gap = if rect.width() < 520. { 3. } else { 7. };
        let controls_left = rect.right() - if custom { caption::WIDTH } else { 0. };
        // 六个工具按钮与两条分隔线，最后一个按钮紧邻窗口控件。
        let split = controls_left - (6. * 36. + 2. + 7. * gap);
        let mut left = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.with_max_x(split - 8.))
                .layout(Layout::left_to_right(Align::Center)),
        );
        left.set_clip_rect(left.clip_rect().intersect(rect.with_max_x(split - 4.)));
        let wide = ui.ctx().screen_rect().width() > 850.;
        left.spacing_mut().item_spacing.x = if wide { 18. } else { 8. };
        if icons::button(&mut left, "back", tr("返回"), 36., 22.).clicked() {
            self.back(wide);
        }
        if !wide
            && icons::selectable_button(
                &mut left,
                "grid",
                tr("主导航"),
                36.,
                20.,
                self.shell.navigation.unwrap_or(false),
            )
            .clicked()
        {
            self.shell.navigation = Some(!self.shell.navigation.unwrap_or(false));
        }
        let drag_left = left.min_rect().right();
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
                .max_rect(rect.with_min_x(split).with_max_x(controls_left))
                .layout(Layout::left_to_right(Align::Center)),
        );
        right.spacing_mut().item_spacing.x = gap;
        self.shell.service_button(&mut right, svc);
        if self.shell.tasks.button(&mut right, svc) {
            self.catalog.artwork = Default::default();
            self.catalog.invalidate_library();
        }
        if icons::selectable_button(
            &mut right,
            "settings",
            tr("设置"),
            36.,
            22.,
            self.screen == Screen::Settings,
        )
        .clicked()
        {
            self.navigate(Screen::Settings);
        }
        caption::separator(&mut right);
        let theme_open = popup_open(&right, "theme-menu");
        let theme = icons::selectable_button(&mut right, "theme", tr("换肤"), 36., 22., theme_open);
        popup(&right, "theme-menu", &theme, 160., |ui| {
            for (key, label) in themes() {
                if style::appearance_choice(ui, prefs.theme == key, tr(label), None).clicked() {
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
        let open = popup_open(&right, "account-menu");
        let motion = style::interaction(&right, &avatar);
        let ink = if open {
            style::highlight(dark)
        } else {
            motion.color(style::text(dark), style::accent(&right), style::text(dark))
        };
        let fill = if open {
            style::accent_soft(&right)
        } else {
            style::surface(dark)
        };
        right.painter().circle(
            r.center(),
            18. - motion.press,
            motion.color(fill, style::hovered(dark), style::accent_soft(&right)),
            egui::Stroke::new(
                1.0_f32,
                if open {
                    style::accent(&right)
                } else {
                    style::line(dark)
                },
            ),
        );
        icons::paint(
            &right,
            "user",
            egui::Rect::from_center_size(r.center(), egui::Vec2::splat(21. - motion.press)),
            ink,
        );
        avatar.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, open, tr("账号菜单"))
        });
        let avatar = avatar.on_hover_text(tr("账号菜单"));
        popup(&right, "account-menu", &avatar, 240., |ui| {
            ui.strong(&svc.server.info.user_name);
            ui.weak(&svc.server.info.endpoint);
            ui.separator();
            if style::button(ui, tr("账号与设置")).clicked() {
                self.navigate(Screen::Settings);
                ui.memory_mut(|m| m.close_popup());
            }
            if style::button(ui, tr("退出登录")).clicked() {
                *action = Some(HostAction::Logout);
                ui.memory_mut(|m| m.close_popup());
            }
        });
        caption::separator(&mut right);
        if icons::selectable_button(
            &mut right,
            "mini",
            tr("迷你播放器"),
            36.,
            21.,
            self.player.mini_open(),
        )
        .clicked()
        {
            self.player.open_mini(right.ctx());
        }
        if custom {
            let title = rect.with_min_x(drag_left).with_max_x(split);
            let top = egui::Rect::from_min_max(
                ui.ctx().screen_rect().min,
                egui::pos2(controls_left, rect.center().y - 18.),
            );
            caption::controls(ui, rect, &[title, top]);
        }
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

    #[test]
    fn mini_player_opens_from_the_header_after_the_account_menu() {
        use egui::accesskit::{Action, ActionRequest, Role};
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let server = runtime.block_on(crate::ui::http_fixture::session(|_| serde_json::json!({})));
        let activity = server.listen_activity(|| {});
        let player =
            musicfs_player_core::Player::new(|_| Err("No media in fixture".into())).unwrap();
        let svc = Services {
            runtime: runtime.handle(),
            server: &server,
            activity: &activity,
            player: &player,
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = SignedUi::default();
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1232., 800.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        app.header(ui, svc, &Preferences::default(), &mut None)
                    });
                },
            )
        };
        let tree = frame(vec![]).platform_output.accesskit_update.unwrap();
        let button = |label| {
            tree.nodes
                .iter()
                .find(|(_, n)| n.role() == Role::Button && n.label() == Some(tr(label)))
                .unwrap()
        };
        let (_, settings) = button("设置");
        let (_, skin) = button("换肤");
        let (_, user) = button("账号菜单");
        let (mini_id, mini) = button("迷你播放器");
        let (settings, skin, user, mini) = (
            settings.bounds().unwrap(),
            skin.bounds().unwrap(),
            user.bounds().unwrap(),
            mini.bounds().unwrap(),
        );
        let usual_gap = user.x0 - skin.x1;
        assert!(usual_gap > 0. && skin.x0 - settings.x1 > usual_gap);
        assert!(mini.x0 - user.x1 > usual_gap);
        frame(vec![egui::Event::AccessKitActionRequest(ActionRequest {
            target: *mini_id,
            action: Action::Click,
            data: None,
        })]);
        assert!(app.player.mini_open());
    }
}
