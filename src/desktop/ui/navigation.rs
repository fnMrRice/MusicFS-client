use super::{data::Services, style, SignedUi};
use eframe::egui::{self, Color32, RichText, Vec2};

#[derive(Clone, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Library,
    Search,
    Recommendations,
    Playlists,
    Playlist(String),
    Favorites,
    History,
    Works(String),
    Settings,
}
impl Screen {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Library => "曲目列表",
            Self::Search => "搜索",
            Self::Recommendations => "推荐",
            Self::Playlists => "我的歌单",
            Self::Playlist(_) => "歌单详情",
            Self::Favorites => "我的收藏",
            Self::History => "最近播放",
            Self::Works(_) => "作品库",
            Self::Settings => "设置",
        }
    }
}

fn entry(ui: &mut egui::Ui, title: &str, symbol: &str, active: bool, nested: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), if nested { 34. } else { 40. }),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    let color = if active {
        Color32::WHITE
    } else {
        ui.visuals().text_color()
    };
    let p = ui.painter_at(rect);
    if active || response.hovered() || response.has_focus() {
        p.rect_filled(
            rect,
            7.,
            if active {
                style::ACCENT
            } else {
                ui.visuals().widgets.hovered.weak_bg_fill
            },
        );
    }
    let center = egui::pos2(rect.left() + 18., rect.center().y);
    super::nav_icons::draw(&p, center, symbol, color);
    let parent = matches!(symbol, "▸" | "▾");
    let text_clip = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.right() - if parent { 26. } else { 7. }, rect.bottom()),
    );
    p.with_clip_rect(text_clip.intersect(ui.clip_rect())).text(
        center + egui::vec2(18., 0.),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(if nested { 14. } else { 15. }),
        color,
    );
    if parent {
        let end = egui::pos2(rect.right() - 15., rect.center().y);
        let vs = if symbol == "▾" {
            [(-3.5, -1.5), (0., 2.), (3.5, -1.5)]
        } else {
            [(-1.5, -3.5), (2., 0.), (-1.5, 3.5)]
        };
        p.add(egui::Shape::line(
            vs.into_iter()
                .map(|(x, y)| end + egui::vec2(x, y))
                .collect(),
            egui::Stroke::new(1.2_f32, color),
        ));
    }
    response.on_hover_text(title).clicked()
}

impl SignedUi {
    pub(super) fn sidebar(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let wide = ctx.screen_rect().width() >= 950.0;
        let dark = ctx.style().visuals.dark_mode;
        egui::SidePanel::left("sidebar")
            .exact_width(if wide { 225.0 } else { 173.0 })
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(style::sidebar(dark))
                    .inner_margin(egui::Margin::symmetric(14, 18)),
            )
            .show(ctx, |ui| {
                ui.add_space(5.0);
                style::note(ui);
                ui.add_space(21.0);
                let mut go = None;
                let mut more = false;
                egui::ScrollArea::vertical()
                    .id_salt("main-navigation")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        for (screen, symbol) in
                            [(Screen::Search, "⌕"), (Screen::Recommendations, "⌂")]
                        {
                            if entry(ui, screen.label(), symbol, self.screen == screen, false) {
                                go = Some(screen);
                            }
                        }
                        if entry(
                            ui,
                            "作品库",
                            if self.works_open { "▾" } else { "▸" },
                            !self.works_open
                                && matches!(self.screen, Screen::Library | Screen::Works(_)),
                            false,
                        ) {
                            self.works_open = !self.works_open;
                        }
                        if self.works_open {
                            ui.indent("work-entries", |ui| {
                                if entry(ui, "曲目列表", "♫", self.screen == Screen::Library, true)
                                {
                                    go = Some(Screen::Library);
                                }
                                for kind in self.browse.types() {
                                    let selected =
                                        matches!(&self.screen,Screen::Works(id) if *id == kind.id);
                                    if entry(ui, &kind.name, &kind.icon, selected, true) {
                                        go = Some(Screen::Works(kind.id.clone()));
                                    }
                                }
                                if let Some(error) = self.browse.types_error() {
                                    if ui.button("重试读取类型").on_hover_text(error).clicked()
                                    {
                                        self.browse.retry_types();
                                    }
                                }
                            });
                        }
                        if entry(ui, "我的歌单", "▤", self.screen == Screen::Playlists, false)
                        {
                            go = Some(Screen::Playlists);
                        }
                        for mine in [true, false] {
                            ui.add_space(11.0);
                            ui.separator();
                            let collapsed = if mine {
                                &mut self.mine_closed
                            } else {
                                &mut self.lists_closed
                            };
                            let label = if mine { "我的" } else { "歌单" };
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(format!(
                                            "{}  {label}",
                                            if *collapsed { "+" } else { "−" }
                                        ))
                                        .color(style::muted(dark)),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                *collapsed = !*collapsed;
                            }
                            if *collapsed {
                                continue;
                            }
                            if mine {
                                for (s, symbol) in
                                    [(Screen::Favorites, "♡"), (Screen::History, "◷")]
                                {
                                    if entry(ui, s.label(), symbol, self.screen == s, false) {
                                        go = Some(s);
                                    }
                                }
                            } else {
                                for list in &self.lists.items {
                                    let selected =
                                        matches!(&self.screen,Screen::Playlist(id) if *id==list.id);
                                    if entry(ui, &list.name, "▤", selected, true) {
                                        go = Some(Screen::Playlist(list.id.clone()));
                                    }
                                }
                                if self.lists.items.is_empty()
                                    && !self.lists.busy()
                                    && self.lists.error.is_none()
                                {
                                    ui.weak("歌单列表为空");
                                }
                                if self.lists.error.is_some() || self.lists.next.is_some() {
                                    more = ui
                                        .add_enabled(
                                            !self.lists.busy(),
                                            egui::Button::new("继续读取歌单"),
                                        )
                                        .clicked();
                                }
                            }
                        }
                    });
                if let Some(screen) = go {
                    self.navigate(screen);
                }
                if more {
                    self.load_lists(ctx, svc);
                }
            });
    }
}
