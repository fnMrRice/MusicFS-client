use super::{label, Folder, Folders, Providers};
use crate::{i18n::tr, icons, ui::style};
use eframe::egui::{self, Align, FontId, Layout, RichText, Stroke, Ui};

pub(super) fn ancestors(ui: &mut Ui, path: &[Folder], providers: &Providers) -> Option<usize> {
    let (_, parents) = path.split_last()?;
    let mut go = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.;
        if style::link(ui, tr("音乐目录")).clicked() {
            go = Some(0);
        }
        if let Some((parent, earlier)) = parents.split_last() {
            ui.weak("›");
            if !earlier.is_empty() {
                let menu = style::menu_button(ui, "…", |ui| {
                    ui.set_max_width(280.);
                    for (index, folder) in earlier.iter().enumerate() {
                        let name = label(folder, providers);
                        if style::add_button(ui, true, egui::Button::new(name).truncate())
                            .on_hover_text(name)
                            .clicked()
                        {
                            go = Some(index + 1);
                            ui.close_menu();
                        }
                    }
                });
                let response = menu.response;
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label(tr("上级文件夹"));
                });
                response.on_hover_text(tr("上级文件夹"));
                ui.weak("›");
            }
            let name = label(parent, providers);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            if style::link(ui, name).on_hover_text(name).clicked() {
                go = Some(parents.len());
            }
        }
    });
    go
}

impl Folders {
    pub(super) fn toolbar(&mut self, ui: &mut Ui, providers: &Providers) {
        let width = ui.available_width();
        let controls = width.min(272.);
        let title = self
            .path
            .last()
            .map_or_else(
                || {
                    tr(if self.search.is_empty() {
                        "音乐目录"
                    } else {
                        "匹配的文件夹"
                    })
                },
                |folder| label(folder, providers),
            )
            .to_owned();
        let count = self.children.total.filter(|_| self.path.is_empty());
        let heading = |ui: &mut Ui| {
            let mut text = egui::text::LayoutJob::default();
            let dark = ui.visuals().dark_mode;
            text.append(
                &title,
                0.,
                egui::TextFormat::simple(style::bold(ui, 21.), style::text(dark)),
            );
            if let Some(count) = count {
                let summary = tr("{0} 个文件夹").replace("{0}", &count.to_string());
                text.append(
                    &summary,
                    12.,
                    egui::TextFormat::simple(FontId::proportional(13.), style::muted(dark)),
                );
            }
            ui.add(egui::Label::new(text).truncate())
                .on_hover_text(&title);
        };
        if width < 510. {
            heading(ui);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 36.),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(controls, 36.),
                        Layout::left_to_right(Align::Center),
                        |ui| self.search_controls(ui),
                    );
                },
            );
        } else {
            ui.horizontal(|ui| {
                let left = width - controls - ui.spacing().item_spacing.x;
                ui.allocate_ui_with_layout(
                    egui::vec2(left, 36.),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_width(left);
                        heading(ui);
                    },
                );
                self.search_controls(ui);
            });
        }
        ui.add_space(6.);
    }

    fn search_controls(&mut self, ui: &mut Ui) {
        let dark = ui.visuals().dark_mode;
        let hint = tr(if self.path.is_empty() {
            "搜索文件夹"
        } else {
            "搜索目录内曲目"
        });
        let clearable = !self.draft.is_empty() || !self.search.is_empty();
        let width = (ui.available_width() - 36. - ui.spacing().item_spacing.x).max(1.);
        let mut submit = false;
        let search = egui::Frame::new()
            .corner_radius(8)
            .stroke(Stroke::new(1.0_f32, style::choice_border(dark)))
            .fill(style::input(dark))
            .inner_margin(egui::Margin::symmetric(3, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    ui.set_width((width - 8.).max(1.));
                    submit = icons::button(ui, "search", tr("搜索"), 32., 16.).clicked();
                    let edit_width =
                        (ui.available_width() - if clearable { 30. } else { 0. }).max(1.);
                    let input = ui.add_sized(
                        [edit_width, 34.],
                        egui::TextEdit::singleline(&mut self.draft)
                            .frame(false)
                            .margin(egui::Margin::ZERO)
                            .vertical_align(Align::Center)
                            .char_limit(256)
                            .hint_text(hint),
                    );
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                        input.request_focus();
                    }
                    submit |= (input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || (input.changed() && self.draft.is_empty());
                    if clearable && icons::button(ui, "close", tr("清空搜索"), 28., 14.).clicked()
                    {
                        self.draft.clear();
                        input.request_focus();
                        submit = true;
                    }
                    input.has_focus()
                })
                .inner
            });
        if search.inner {
            ui.painter().rect_stroke(
                search.response.rect,
                8,
                Stroke::new(1.25_f32, style::accent(ui)),
                egui::StrokeKind::Inside,
            );
        }
        if submit && self.search != self.draft.trim() {
            self.search = self.draft.trim().into();
            if self.path.is_empty() {
                self.children = Default::default();
            }
            self.tracks = Default::default();
            self.all = None;
            self.message = None;
        }
        if icons::button(ui, "refresh", tr("刷新列表"), 36., 18.).clicked() {
            self.reload();
        }
    }
}

pub(super) fn section(ui: &mut Ui, title: &str, total: Option<usize>) {
    ui.label(RichText::new(title).font(style::bold(ui, 16.)));
    if let Some(count) = total {
        ui.weak(count.to_string());
    }
}

pub(super) fn empty(ui: &mut Ui, icon: &str, title: &str) {
    ui.add_space(24.);
    ui.vertical_centered(|ui| {
        let (tile, _) = ui.allocate_exact_size(egui::Vec2::splat(46.), egui::Sense::hover());
        ui.painter()
            .circle_filled(tile.center(), 23., style::accent_soft(ui));
        icons::paint(ui, icon, tile.shrink(12.), style::accent(ui));
        ui.add_space(4.);
        ui.label(RichText::new(title).color(style::muted(ui.visuals().dark_mode)));
    });
}
