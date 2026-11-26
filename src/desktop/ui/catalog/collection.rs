use super::*;
use egui::{Align2, Color32, FontId, Rect, Sense};

#[cfg(test)]
#[path = "../../../../tests/native/category_viewport.rs"]
mod viewport_tests;

impl Catalog {
    pub(super) fn album_filters(&mut self, ui: &mut egui::Ui) {
        ui.scope(|ui| {
            style::checkbox_style(ui);
            if ui
                .checkbox(&mut self.unscraped_only, tr("仅显示未刮削专辑"))
                .changed()
            {
                self.categories = Default::default();
            }
        });
        for (mode, icon, label) in [(false, "grid", "卡片视图"), (true, "queue", "列表视图")]
        {
            let b = crate::icons::button(ui, icon, tr(label), 32., 16.);
            b.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    true,
                    self.album_list == mode,
                    tr(label),
                )
            });
            if b.clicked() {
                self.album_list = mode;
            }
            if self.album_list == mode {
                ui.painter().hline(
                    b.rect.shrink(4.).x_range(),
                    b.rect.bottom() - 1.,
                    egui::Stroke::new(2.0_f32, style::accent(ui)),
                );
            }
        }
    }

    pub(super) fn collection(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
    ) -> Option<(String, String)> {
        let albums = self.tabs == CategoryTab::Album;
        let list = albums && self.album_list;
        let width = ui.available_width();
        let cols = if list {
            1
        } else {
            (width / 200.).max(1.) as usize
        };
        let card = (width - (cols - 1) as f32 * ui.spacing().item_spacing.x) / cols as f32;
        let mut selected = None;
        if list && !self.categories.items.is_empty() {
            let (r, _) = ui.allocate_exact_size(egui::vec2(card, 32.), Sense::hover());
            let fg = style::muted(ui.visuals().dark_mode);
            field(ui, r, 12., card * 0.46, tr("专辑名称"), fg);
            if width >= 640. {
                field(ui, r, card * 0.51, card * 0.18, tr("发行商"), fg);
                field(ui, r, card * 0.72, card * 0.16, tr("发行日期"), fg);
            }
            field(ui, r, card * 0.89, card * 0.11, tr("曲目数"), fg);
        }
        let card_size = egui::vec2(card, if list { 55. } else { 68. });
        category_cards(
            ui,
            self.categories.items.len(),
            cols,
            card_size,
            |ui, index, r, response| {
                let item = &self.categories.items[index];
                let name = if item.name.is_empty() {
                    tr("未知{0}").replace("{0}", self.tabs.label())
                } else {
                    item.name.clone()
                };
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name)
                });
                if response.clicked() {
                    selected = Some((item.id.clone(), item.name.clone()));
                }
                if !ui.is_rect_visible(r) {
                    return;
                }
                let dark = ui.visuals().dark_mode;
                let fg = ui.visuals().text_color();
                let secondary = style::muted(dark);
                let fill = if response.hovered() || response.has_focus() {
                    style::hovered(dark)
                } else {
                    style::surface(dark)
                };
                ui.painter().rect_filled(r, if list { 0 } else { 8 }, fill);
                if list {
                    ui.painter().hline(
                        r.x_range(),
                        r.bottom(),
                        egui::Stroke::new(1.0_f32, style::line(dark)),
                    );
                } else {
                    ui.painter().rect_stroke(
                        r,
                        8,
                        egui::Stroke::new(1.0_f32, style::line(dark)),
                        egui::StrokeKind::Inside,
                    );
                }
                let cover = Rect::from_center_size(
                    egui::pos2(r.left() + 32., r.center().y),
                    egui::Vec2::splat(42.),
                );
                if albums {
                    self.artwork
                        .paint(ui, cover, svc, item.cover_object_id.as_deref());
                } else {
                    crate::icons::paint(
                        ui,
                        if self.tabs == CategoryTab::Artist {
                            "user"
                        } else {
                            "music"
                        },
                        cover.shrink(8.),
                        secondary,
                    );
                }
                if list {
                    field(
                        ui,
                        r,
                        64.,
                        if width >= 640. {
                            card * 0.50 - 68.
                        } else {
                            card * 0.86 - 68.
                        },
                        &name,
                        fg,
                    );
                    if width >= 640. {
                        field(
                            ui,
                            r,
                            card * 0.51,
                            card * 0.18,
                            item.publisher.as_deref().unwrap_or("—"),
                            secondary,
                        );
                        field(
                            ui,
                            r,
                            card * 0.72,
                            card * 0.16,
                            item.release_date.as_deref().unwrap_or("—"),
                            secondary,
                        );
                    }
                    field(
                        ui,
                        r,
                        card * 0.89,
                        card * 0.11,
                        &item.track_count.to_string(),
                        secondary,
                    );
                } else {
                    field(
                        ui,
                        r.translate(egui::vec2(0., -10.)),
                        64.,
                        card - 70.,
                        &name,
                        fg,
                    );
                    field(
                        ui,
                        r.translate(egui::vec2(0., 11.)),
                        64.,
                        card - 70.,
                        &tr("{0} 首").replace("{0}", &item.track_count.to_string()),
                        secondary,
                    );
                }
                response.on_hover_text(name);
            },
        );
        selected
    }
}

pub(crate) fn category_cards(
    ui: &mut egui::Ui,
    count: usize,
    cols: usize,
    size: egui::Vec2,
    mut show: impl FnMut(&egui::Ui, usize, Rect, egui::Response),
) {
    if count == 0 {
        return;
    }
    let rows = count.div_ceil(cols);
    let spacing = ui.spacing().item_spacing;
    let stride = size + spacing;
    let (id, body) = ui.allocate_space(egui::vec2(
        ui.available_width(),
        rows as f32 * stride.y - spacing.y,
    ));
    // 预留整段滚动高度，仅为视口及上下各一行建立控件，以便键盘焦点能继续移动。
    let clip = ui.clip_rect();
    let first = ((clip.top() - body.top()) / stride.y).floor() as usize;
    let end = ((clip.bottom() - body.top()) / stride.y).ceil() as usize;
    for row in first.saturating_sub(1)..end.saturating_add(1).min(rows) {
        let start = row * cols;
        for index in start..(start + cols).min(count) {
            let x = body.left() + (index - start) as f32 * stride.x;
            let y = body.top() + row as f32 * stride.y;
            let rect = Rect::from_min_size(egui::pos2(x, y), size);
            let response = ui.interact(rect, id.with(index), Sense::click());
            if response.gained_focus() {
                response.scroll_to_me(None);
            }
            show(ui, index, rect, response);
        }
    }
}

fn field(ui: &egui::Ui, area: Rect, offset: f32, width: f32, text: &str, fg: Color32) {
    let start = egui::pos2(area.left() + offset, area.center().y);
    let clip = Rect::from_min_max(
        egui::pos2(start.x, area.top()),
        egui::pos2(start.x + width.max(0.), area.bottom()),
    );
    ui.painter()
        .with_clip_rect(clip.intersect(ui.clip_rect()))
        .text(
            start,
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(14.),
            fg,
        );
}
