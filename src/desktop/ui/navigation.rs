use super::{data::Services, style, SignedUi};
use eframe::egui::{self, Align, Color32, RichText, Vec2};

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
    let text = RichText::new(format!("{symbol}    {title}")).size(if nested { 14.0 } else { 15.0 });
    let button = egui::Button::new(if active {
        text.color(Color32::WHITE)
    } else {
        text
    })
    .corner_radius(7)
    .stroke(egui::Stroke::NONE)
    .fill(if active {
        style::ACCENT
    } else {
        Color32::TRANSPARENT
    })
    .min_size(Vec2::new(0.0, if nested { 34.0 } else { 40.0 }))
    .truncate();
    ui.add_sized(
        [ui.available_width(), if nested { 34.0 } else { 40.0 }],
        button,
    )
    .on_hover_text(title)
    .clicked()
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
                                    if entry(ui, &kind.name, "◉", selected, true) {
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
                                            if *collapsed { "▸" } else { "▾" }
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
                        ui.with_layout(egui::Layout::top_down(Align::Min), |_ui| {});
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
