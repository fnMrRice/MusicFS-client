use super::{
    layout::{Columns, HEADER, HEIGHT},
    style,
};
use eframe::egui::{self, pos2, Align, Layout, Rect, Response, Sense, Ui, Vec2};

pub(super) fn width(ui: &Ui) -> f32 {
    let screen = ui.ctx().screen_rect();
    let right_margin = if screen.width() <= 850. { 18. } else { 30. };
    (screen.right() - right_margin - ui.next_widget_position().x)
        .min(ui.available_width())
        .max(0.)
}

pub(super) fn show(
    ui: &mut Ui,
    scroll: egui::ScrollArea,
    width: f32,
    columns: impl FnOnce(f32) -> Columns,
    rows: usize,
    mut paint: impl FnMut(&mut Ui, Rect, &Columns, Option<usize>),
) {
    let stroke = egui::Stroke::new(1.0_f32, style::line(ui.visuals().dark_mode));
    egui::Frame::new()
        .stroke(stroke)
        .corner_radius(9)
        .inner_margin(0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let width = (width - 2.).min(ui.available_width()).max(0.);
            ui.set_width(width);
            let columns = columns(width);
            let (header, _) = ui.allocate_exact_size(egui::vec2(width, HEADER), Sense::hover());
            paint(ui, header, &columns, None);
            ui.painter()
                .hline(header.x_range(), header.bottom(), stroke);
            scroll
                .auto_shrink([false, true])
                .show_rows(ui, HEIGHT, rows, |ui, range| {
                    for index in range {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(width.min(ui.available_width()), HEIGHT),
                            Sense::hover(),
                        );
                        let mut cell = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt(("track-slot", index))
                                .max_rect(rect)
                                .layout(Layout::left_to_right(Align::Center)),
                        );
                        paint(&mut cell, rect, &columns, Some(index));
                        if index + 1 < rows {
                            ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
                        }
                    }
                });
        });
}

pub(super) fn row_background(ui: &mut Ui, rect: Rect, marked: bool) -> Response {
    let dark = ui.visuals().dark_mode;
    let row = ui.interact(rect, ui.id().with("row"), Sense::click());
    let motion = style::interaction_with_hover(ui, &row, ui.rect_contains_pointer(rect));
    let base = if marked {
        style::accent_soft(ui)
    } else {
        style::background(dark)
    };
    let hover = if marked {
        style::hovered(dark).lerp_to_gamma(style::accent_fill(ui), 0.10)
    } else {
        style::hovered(dark)
    };
    let pressed = hover.lerp_to_gamma(style::accent_fill(ui), 0.15);
    ui.painter()
        .rect_filled(rect, 0, motion.color(base, hover, pressed));
    row
}

pub(super) fn title_rects(title: Rect) -> (Rect, Rect, Rect) {
    let left = title.left() + if title.width() > 150. { 14. } else { 4. };
    let cover = Rect::from_center_size(pos2(left + 21., title.center().y), Vec2::splat(42.));
    let tx = cover.right() + if title.width() > 170. { 12. } else { 5. };
    let line = |y: f32| Rect::from_min_max(pos2(tx, y - 11.), pos2(title.right() - 3., y + 11.));
    (
        cover,
        line(title.center().y - 9.),
        line(title.center().y + 12.),
    )
}
