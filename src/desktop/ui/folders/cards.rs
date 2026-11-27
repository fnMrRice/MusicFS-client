use super::{label, Folder, Providers};
use crate::ui::{catalog::card_grid, style};
use crate::{i18n::tr, icons};
use eframe::egui::{self, Color32, FontId, Rect, Stroke, StrokeKind};

pub(super) fn grid(ui: &mut egui::Ui, folders: &[Folder], providers: &Providers) -> Option<usize> {
    let width = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let columns = ((width + gap) / (246. + gap)).floor().max(1.) as usize;
    let size = egui::vec2(((width + gap) / columns as f32 - gap).max(1.), 74.);
    let mut opened = None;
    card_grid(
        ui,
        folders.len(),
        columns,
        size,
        |ui, index, rect, response| {
            let folder = &folders[index];
            let name = label(folder, providers);
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
            if response.clicked() {
                opened = Some(index);
            }
            if !ui.is_rect_visible(rect) {
                return;
            }

            let dark = ui.visuals().dark_mode;
            let motion = style::interaction(ui, &response);
            let fill = motion.color(
                style::surface(dark),
                style::hovered(dark),
                style::accent_soft(ui),
            );
            let edge = if response.has_focus() {
                Stroke::new(2.0_f32, style::accent(ui))
            } else {
                Stroke::new(1.0_f32, style::line(dark))
            };
            let rect = rect.shrink(motion.press);
            ui.painter().rect(rect, 9, fill, edge, StrokeKind::Inside);
            let content = rect.shrink(12.);
            let mut left = content.left();
            if content.width() >= 130. {
                let tile = Rect::from_center_size(
                    egui::pos2(left + 21., content.center().y),
                    egui::Vec2::splat(42.),
                );
                ui.painter().rect_filled(tile, 9, style::accent_soft(ui));
                icons::paint(ui, "folder", tile.shrink(9.), style::accent(ui));
                left = tile.right() + 12.;
            }

            let count = tr("{0} 首").replace("{0}", &folder.track_count.to_string());
            let detail = format!(
                "{count} · {}",
                providers.label(&folder.provider_instance_id)
            );
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_description(detail.as_str());
            });
            let text_area = Rect::from_min_max(egui::pos2(left, content.top()), content.max);
            line(
                ui,
                text_area,
                -11.,
                name,
                style::bold(ui, 15.),
                style::text(dark),
            );
            line(
                ui,
                text_area,
                11.,
                &detail,
                FontId::proportional(12.),
                style::muted(dark),
            );
            response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("{name}\n{detail}"));
        },
    );
    opened
}

pub(super) fn line(
    ui: &egui::Ui,
    area: Rect,
    offset: f32,
    text: &str,
    font: FontId,
    color: Color32,
) {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, area.width().max(1.));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let top = area.left_center() + egui::vec2(0., offset - galley.size().y / 2.);
    ui.painter_at(area).galley(top, galley, color);
}
