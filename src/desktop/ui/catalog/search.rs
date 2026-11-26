use super::*;
use crate::icons;
use egui::{Align, Button, FontId, Frame, Layout, Margin, Sense, Stroke, TextEdit, Vec2};

#[cfg(test)]
#[path = "../../../../tests/native/library_search_input.rs"]
mod input_tests;

pub(crate) enum SearchAction {
    Stay,
    Tracks(TrackQuery),
    OpenCategory,
}

#[derive(Default)]
pub(super) struct LibrarySearch {
    draft: String,
    query: String,
    recent: Vec<String>,
    section: CategoryTab,
    results: Pages<Category>,
    invalid: bool,
}

impl LibrarySearch {
    fn submit(&mut self) -> bool {
        let q = self.draft.trim();
        self.invalid = q.chars().count() > 256 || q.chars().any(char::is_control);
        if q.is_empty() || self.invalid {
            return false;
        }
        self.query = q.to_owned();
        self.draft = self.query.clone();
        self.recent.retain(|s| s != &self.query);
        self.recent.insert(0, self.query.clone());
        self.recent.truncate(8);
        self.results = Pages::default();
        true
    }

    fn clear(&mut self) {
        self.draft.clear();
        self.query.clear();
        self.invalid = false;
        self.results = Pages::default();
    }

    fn form(&mut self, ui: &mut egui::Ui, focus: &mut bool) -> bool {
        let dark = ui.visuals().dark_mode;
        let mut submit = false;
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.;
            let width = (ui.available_width() - 82. - ui.spacing().item_spacing.x).max(140.);
            let field = Frame::new()
                .fill(style::input(dark))
                .stroke(Stroke::new(1.0_f32, style::choice_border(dark)))
                .corner_radius(10)
                .inner_margin(Margin::symmetric(12, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_width((width - 26.).max(100.));
                        ui.set_height(34.);
                        let (icon, _) = ui.allocate_exact_size(Vec2::splat(19.), Sense::hover());
                        icons::paint(ui, "search", icon, style::muted(dark));
                        let show_clear = !self.draft.is_empty() || !self.query.is_empty();
                        let w = (ui.available_width() - if show_clear { 38. } else { 0. }).max(60.);
                        let input = ui.add_sized(
                            [w, 34.],
                            TextEdit::singleline(&mut self.draft)
                                .frame(false)
                                .margin(Margin::ZERO)
                                .vertical_align(Align::Center)
                                .font(FontId::proportional(16.))
                                .hint_text(tr("搜索曲名、歌手或专辑")),
                        );
                        if std::mem::take(focus) {
                            input.request_focus();
                        }
                        if input.changed() {
                            self.invalid = false;
                        }
                        submit =
                            input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if show_clear
                            && icons::button(ui, "close", tr("清空搜索"), 28., 14.).clicked()
                        {
                            clear = true;
                            input.request_focus();
                        }
                        input.has_focus()
                    })
                    .inner
                });
            if field.inner {
                ui.painter().rect_stroke(
                    field.response.rect,
                    10,
                    Stroke::new(1.25_f32, style::accent(ui)),
                    egui::StrokeKind::Inside,
                );
            }
            ui.spacing_mut().interact_size.y = field.response.rect.height();
            submit |= style::primary(ui, tr("搜索"), !self.draft.trim().is_empty()).clicked();
        });
        if clear {
            self.clear();
            return true;
        }
        let changed = submit && self.submit();
        if self.invalid {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr("关键词最多 256 个字符，且不能包含控制字符。"),
            );
        }
        changed
    }

    fn recent(&mut self, ui: &mut egui::Ui) -> bool {
        if self.recent.is_empty() {
            empty(
                ui,
                "search",
                tr("搜索音乐库"),
                tr("输入曲名、歌手或专辑名，按 Enter 搜索。"),
            );
            return false;
        }
        ui.add_space(18.);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("最近搜索")).font(style::bold(ui, 17.)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.link(tr("清空记录")).clicked() {
                    self.recent.clear();
                }
            });
        });
        ui.add_space(7.);
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            for q in &self.recent {
                if ui.add(Button::new(q).corner_radius(8)).clicked() {
                    chosen = Some(q.clone());
                }
            }
        });
        if let Some(q) = chosen {
            self.draft = q;
            return self.submit();
        }
        false
    }

    fn tabs(&mut self, ui: &mut egui::Ui) -> bool {
        let previous = self.section;
        let dark = ui.visuals().dark_mode;
        ui.add_space(14.);
        ui.horizontal(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(15., 7.);
            for (tab, key) in [
                (CategoryTab::All, "曲目"),
                (CategoryTab::Album, "专辑"),
                (CategoryTab::Artist, "歌手"),
            ] {
                if ui
                    .selectable_value(&mut self.section, tab, tr(key))
                    .changed()
                {
                    self.results = Default::default();
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(tr("“{0}”的搜索结果").replace("{0}", &self.query))
                            .color(style::muted(dark)),
                    )
                    .truncate(),
                );
            });
        });
        ui.add_space(9.);
        ui.separator();
        ui.add_space(5.);
        previous != self.section
    }
}

fn empty(ui: &mut egui::Ui, icon: &str, title: &str, help: &str) {
    ui.add_space((ui.available_height() * 0.22).min(95.).max(16.));
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 26., style::accent_soft(ui));
        icons::paint(ui, icon, rect.shrink(14.), style::accent(ui));
        ui.add_space(9.);
        ui.label(RichText::new(title).font(style::bold(ui, 18.)));
        ui.add_space(3.);
        ui.weak(help);
    });
}

impl Catalog {
    pub fn search_page(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> SearchAction {
        if self.search.form(ui, &mut self.focus_search) {
            self.reset_tracks();
        }
        if self.search.query.is_empty() {
            if self.search.recent(ui) {
                self.reset_tracks();
            } else {
                return SearchAction::Stay;
            }
        }
        if self.search.tabs(ui) {
            self.reset_tracks();
        }
        let Some(kind) = self.search.section.kind() else {
            let query = TrackQuery::Library(self.search.query.clone());
            self.sync(ui, svc, &query);
            if !self.tracks.items.is_empty() {
                return SearchAction::Tracks(query);
            }
            if self.tracks.busy() {
                loading(ui);
            } else if let Some(err) = &self.tracks.error {
                if retry(ui, err) {
                    self.more(ui, svc);
                }
            } else if self.tracks.next.is_some() {
                self.more(ui, svc);
            } else {
                empty(
                    ui,
                    "search",
                    tr("没有找到相关曲目"),
                    tr("试试其他关键词，或切换搜索类型。"),
                );
            }
            return SearchAction::Stay;
        };
        let search = &mut self.search;
        search.results.poll();
        let mut load = !search.results.started;
        let mut selected = None;
        self.album_cover.poll(&mut self.artwork);
        self.artwork.poll(ui.ctx());
        egui::ScrollArea::vertical()
            .id_salt(("search-categories", search.section.label(), &search.query))
            .show(ui, |ui| {
                selected = Self::category_collection(
                    ui,
                    svc,
                    &search.results.items,
                    search.section,
                    false,
                    &mut self.artwork,
                );
                if let Some(err) = &search.results.error {
                    load |= retry(ui, err);
                } else if search.results.busy() || !search.results.started {
                    loading(ui);
                } else if search.results.next.is_some() {
                    load |= crate::ui::page_end_visible(ui);
                } else if search.results.items.is_empty() {
                    let title = if search.section == CategoryTab::Album {
                        tr("没有找到相关专辑")
                    } else {
                        tr("没有找到相关歌手")
                    };
                    empty(ui, "search", title, tr("试试其他关键词，或切换搜索类型。"));
                }
            });
        if load {
            let server = svc.server.clone();
            let q = search.query.clone();
            let cursor = search.results.next.clone();
            search.results.load(ui.ctx(), svc.runtime, async move {
                server.categories(kind, None, &q, cursor.as_deref()).await
            });
        }
        if let Some((id, name)) = selected {
            let tab = search.section;
            self.open_credit(tab, id, name);
            SearchAction::OpenCategory
        } else {
            SearchAction::Stay
        }
    }
}

fn loading(ui: &mut egui::Ui) {
    ui.add_space(9.);
    ui.horizontal(|ui| {
        ui.spinner();
        ui.weak(tr("正在搜索…"));
    });
}

fn retry(ui: &mut egui::Ui, error: &str) -> bool {
    ui.colored_label(ui.visuals().error_fg_color, error);
    ui.button(tr("重试")).clicked()
}
