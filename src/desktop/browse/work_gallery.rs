use crate::{
    i18n::tr,
    icons,
    ui::{card_grid, style},
};
use eframe::egui::{self, Color32, FontId, Rect, RichText, Stroke, StrokeKind};
use musicfs_server_adapter::{Work, WorkType};

pub(super) fn type_icon(kind: &WorkType) -> &str {
    match kind.icon.as_str() {
        "disc" | "headphones" | "radio" => &kind.icon,
        _ => "album",
    }
}

pub(super) fn mark(ui: &egui::Ui, rect: Rect, icon: &str) {
    ui.painter().rect_filled(rect, 9, style::accent_soft(ui));
    let glyph = Rect::from_center_size(rect.center(), rect.size() * 0.55);
    icons::paint(ui, icon, glyph, style::accent(ui));
}

pub(super) fn panel(ui: &egui::Ui) -> egui::Frame {
    let dark = ui.visuals().dark_mode;
    egui::Frame::new()
        .fill(style::surface(dark))
        .stroke(Stroke::new(1_f32, style::line(dark)))
        .corner_radius(10)
        .inner_margin(18)
}

pub(super) fn message(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    note: &str,
    action: Option<&str>,
) -> bool {
    panel(ui)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(16.);
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(52.), egui::Sense::hover());
                mark(ui, rect, icon);
                ui.add_space(4.);
                ui.label(RichText::new(title).font(style::bold(ui, 20.)));
                if !note.is_empty() {
                    ui.label(RichText::new(note).color(style::muted(ui.visuals().dark_mode)));
                }
                let clicked = action.is_some_and(|label| {
                    ui.add_space(4.);
                    ui.button(label).clicked()
                });
                ui.add_space(16.);
                clicked
            })
            .inner
        })
        .inner
}

pub(super) fn cards(ui: &mut egui::Ui, items: &[Work], kind: &WorkType) -> Option<String> {
    let width = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let cols = ((width + gap) / (300. + gap)).floor().clamp(1., 4.) as usize;
    let cell = egui::vec2(
        ((width - (cols - 1) as f32 * gap) / cols as f32).max(1.),
        128.,
    );
    let dark = ui.visuals().dark_mode;
    let mut chosen = None;
    card_grid(ui, items.len(), cols, cell, |ui, i, rect, response| {
        let w = &items[i];
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &w.title));
        if response.clicked() {
            chosen = Some(w.work_id.clone());
        }
        if !ui.is_rect_visible(rect) {
            return;
        }
        let fill = if response.hovered() || response.has_focus() {
            style::hovered(dark)
        } else {
            style::surface(dark)
        };
        let border = if response.has_focus() {
            style::accent(ui)
        } else {
            style::line(dark)
        };
        ui.painter()
            .rect(rect, 9, fill, Stroke::new(1_f32, border), StrokeKind::Inside);
        let inner = rect.shrink(14.);
        let icon = Rect::from_min_size(inner.min, egui::Vec2::splat(40.));
        mark(ui, icon, type_icon(kind));
        let from = egui::pos2(icon.right() + 10., inner.top());
        let header = Rect::from_min_max(from, egui::pos2(inner.right(), inner.top() + 52.));
        let height = lines(
            ui,
            header,
            &w.title,
            style::bold(ui, 16.),
            style::text(dark),
            2,
        );
        if let Some(id) = w.external_ids.first() {
            let at = egui::pos2(from.x, from.y + height + 2.);
            lines(
                ui,
                Rect::from_min_max(at, header.max),
                &id.value,
                FontId::proportional(12.),
                style::muted(dark),
                1,
            );
        }
        let mut y = inner.top() + 56.;
        if !w.credits.is_empty() {
            let credits = w
                .credits
                .iter()
                .take(3)
                .map(|c| {
                    let role = tr(kind.role_label(&c.role));
                    if role.is_empty() {
                        c.name.clone()
                    } else {
                        format!("{role}  {}", c.name)
                    }
                })
                .collect::<Vec<_>>()
                .join("   ·   ");
            let area = Rect::from_min_max(egui::pos2(inner.left(), y), inner.max);
            y += lines(
                ui,
                area,
                &credits,
                FontId::proportional(13.),
                style::text(dark),
                1,
            ) + 5.;
        }
        let area = Rect::from_min_max(egui::pos2(inner.left(), y), inner.max);
        if !w.tags.is_empty() {
            lines(
                ui,
                area,
                &w.tags.join("   ·   "),
                FontId::proportional(12.),
                style::accent(ui),
                1,
            );
        } else if !w.description.is_empty() {
            lines(
                ui,
                area,
                &w.description,
                FontId::proportional(12.),
                style::muted(dark),
                if w.credits.is_empty() { 2 } else { 1 },
            );
        }
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(&w.title);
    });
    chosen
}

pub(super) fn lines(
    ui: &egui::Ui,
    rect: Rect,
    text: &str,
    font: FontId,
    color: Color32,
    rows: usize,
) -> f32 {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, rect.width().max(1.));
    job.wrap.max_rows = rows;
    let galley = ui.fonts(|f| f.layout_job(job));
    let height = galley.size().y;
    ui.painter_at(rect).galley(rect.min, galley, color);
    height
}
