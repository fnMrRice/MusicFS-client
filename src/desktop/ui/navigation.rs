use super::{data::Services, style, SignedUi};
use crate::i18n::tr;
use eframe::egui::{self, Vec2};

#[derive(Clone, PartialEq, Eq)]
pub struct SourcePlaylist {
    pub instance: String,
    pub account: Option<String>,
    pub external: String,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Library,
    Search,
    Recommendations,
    Playlists,
    Playlist(String),
    SourcePlaylist(SourcePlaylist),
    Favorites,
    History,
    Works(String),
    Settings,
}
impl Screen {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Library => tr("曲目列表"),
            Self::Search => tr("搜索"),
            Self::Recommendations => tr("推荐"),
            Self::Playlists => tr("歌单管理"),
            Self::Playlist(_) | Self::SourcePlaylist(_) => tr("歌单详情"),
            Self::Favorites => tr("我的收藏"),
            Self::History => tr("最近播放"),
            Self::Works(_) => tr("作品库"),
            Self::Settings => tr("设置"),
        }
    }
}

pub(super) fn entry(
    ui: &mut egui::Ui,
    title: &str,
    symbol: &str,
    active: bool,
    nested: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), if nested { 39. } else { 46. }),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    let parent = matches!(symbol, "▸" | "▾");
    let within = nested || parent;
    let color = if active {
        if within {
            style::accent(ui)
        } else {
            style::on_accent(ui)
        }
    } else {
        ui.visuals().text_color()
    };
    let p = ui.painter_at(rect);
    if active || response.hovered() || response.has_focus() {
        p.rect_filled(
            rect,
            7.,
            if active {
                if within {
                    style::accent_soft(ui)
                } else {
                    style::accent_fill(ui)
                }
            } else {
                ui.visuals().widgets.hovered.weak_bg_fill
            },
        );
    }
    if active && !parent {
        let marker = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 2., rect.center().y),
            egui::vec2(3., 18.),
        );
        p.rect_filled(marker, 2., style::accent_fill(ui));
    }
    let center = egui::pos2(rect.left() + 25., rect.center().y);
    let icon = match symbol {
        "⌕" => "search",
        "⌂" => "home",
        "♫" => "music",
        "▾" | "▸" => "album",
        "◉" | "disc" => "album",
        "♡" => "heart",
        "◷" => "clock",
        "▤" => "folder",
        "headphones" => "headphones",
        "radio" => "radio",
        _ => "album",
    };
    crate::icons::paint(
        ui,
        icon,
        egui::Rect::from_center_size(center, Vec2::splat(20.)),
        color,
    );
    let text_clip = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.right() - if parent { 26. } else { 7. }, rect.bottom()),
    );
    p.with_clip_rect(text_clip.intersect(ui.clip_rect())).text(
        center + egui::vec2(24., 0.),
        egui::Align2::LEFT_CENTER,
        title,
        if active && nested {
            style::bold(ui, 14.)
        } else {
            egui::FontId::proportional(if nested { 14. } else { 15. })
        },
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
        let wide = ctx.screen_rect().width() > 850.0;
        let dark = ctx.style().visuals.dark_mode;
        egui::SidePanel::left("sidebar")
            .exact_width(if wide { 240.0 } else { 190.0 })
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(style::sidebar(dark))
                    .inner_margin(egui::Margin::symmetric(if wide { 16 } else { 8 }, 0)),
            )
            .show(ctx, |ui| {
                let side = ui.max_rect();
                ui.painter().vline(
                    side.right() + if wide { 15. } else { 7. },
                    side.y_range(),
                    egui::Stroke::new(1.0_f32, style::line(dark)),
                );
                let top = ui.cursor().top();
                ui.add_space(26.);
                let brand = ui.horizontal(|ui| {
                    ui.add_space(12.);
                    style::note(ui);
                });
                let link = ui.interact(
                    brand.response.rect,
                    ui.make_persistent_id("library-home"),
                    egui::Sense::click(),
                );
                link.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "MusicFS")
                });
                if link.clicked() {
                    self.navigate(Screen::Library);
                }
                ui.add_space((top + if wide { 104. } else { 88. } - ui.cursor().top()).max(0.));
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
                            tr("作品库"),
                            if self.works_closed { "▸" } else { "▾" },
                            matches!(self.screen, Screen::Library | Screen::Works(_)),
                            false,
                        ) {
                            self.works_closed = !self.works_closed;
                        }
                        if !self.works_closed {
                            ui.indent("work-entries", |ui| {
                                if entry(
                                    ui,
                                    tr("曲目列表"),
                                    "♫",
                                    self.screen == Screen::Library,
                                    true,
                                ) {
                                    go = Some(Screen::Library);
                                }
                                for kind in self.browse.types() {
                                    let selected =
                                        matches!(&self.screen,Screen::Works(id) if *id == kind.id);
                                    if entry(ui, tr(&kind.name), &kind.icon, selected, true) {
                                        go = Some(Screen::Works(kind.id.clone()));
                                    }
                                }
                                if let Some(error) = self.browse.types_error() {
                                    if ui.button(tr("重试读取类型")).on_hover_text(error).clicked()
                                    {
                                        self.browse.retry_types();
                                    }
                                }
                            });
                        }
                        if entry(
                            ui,
                            tr("歌单管理"),
                            "▤",
                            self.screen == Screen::Playlists,
                            false,
                        ) {
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
                            let label = if mine { tr("我的") } else { tr("歌单") };
                            if group_button(ui, label, *collapsed) {
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
                                if let Some(error) = &self.lists.error {
                                    more = ui
                                        .button(tr("重试读取歌单"))
                                        .on_hover_text(error)
                                        .clicked();
                                } else if self.lists.can_load_more() {
                                    more = super::page_end_visible(ui);
                                }
                                let empty = self.playlists.sidebar(ui, svc, &self.screen, &mut go);
                                if empty
                                    && self.lists.items.is_empty()
                                    && self.lists.next.is_none()
                                    && !self.lists.busy()
                                    && self.lists.error.is_none()
                                {
                                    ui.weak(tr("歌单列表为空"));
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

#[cfg(test)]
#[test]
fn selection_marker_belongs_to_the_destination() {
    let ctx = egui::Context::default();
    style::apply(&ctx, "dark", "miku");
    for (symbol, nested, expected) in [("▾", false, false), ("▸", false, false), ("♫", true, true)]
    {
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                entry(ui, "Library", symbol, true, nested);
            });
        });
        let marker = output.shapes.iter().any(|s| {
            matches!(&s.shape,
                egui::Shape::Rect(r) if r.rect.width() == 3. && r.rect.height() == 18.
            )
        });
        assert_eq!(marker, expected, "Unexpected selection marker for {symbol}");
    }
}

fn group_button(ui: &mut egui::Ui, label: &str, collapsed: bool) -> bool {
    let (r, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 29.), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let ink = style::muted(ui.visuals().dark_mode);
    let center = egui::pos2(r.left() + 12., r.center().y);
    let points = if collapsed {
        [(-2., -4.), (2., 0.), (-2., 4.)]
    } else {
        [(-4., -2.), (0., 2.), (4., -2.)]
    };
    ui.painter().add(egui::Shape::line(
        Vec::from(points.map(|(x, y)| center + egui::vec2(x, y))),
        egui::Stroke::new(1.4_f32, ink),
    ));
    ui.painter().text(
        center + egui::vec2(17., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.),
        ink,
    );
    response.clicked()
}
