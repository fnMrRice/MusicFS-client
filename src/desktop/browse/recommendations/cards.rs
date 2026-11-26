use super::detail::Detail;
use crate::{
    i18n::tr,
    ui::{card_grid, style},
};
use eframe::egui::{self, Color32, FontId, Rect, Stroke, StrokeKind};
use musicfs_server_adapter::Source;

#[cfg(test)]
#[path = "../../../../tests/native/recommendation_cards.rs"]
mod tests;

pub(super) fn show(
    ui: &mut egui::Ui,
    source: &Source,
    mut paint_cover: impl FnMut(&egui::Ui, Rect, Option<&str>),
) -> Option<Detail> {
    let width = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let cols = ((width + gap) / (230. + gap)).floor().clamp(1., 4.) as usize;
    let card_width = ((width - (cols - 1) as f32 * gap) / cols as f32).max(1.);
    let mut selected = None;
    card_grid(
        ui,
        source.items.len(),
        cols,
        egui::vec2(card_width, 94.),
        |ui, index, rect, response| {
            let item = &source.items[index];
            let playlist = &item.playlist;
            let title = &playlist.title;
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
            if response.clicked() {
                selected = Some(Detail::new(
                    &source.instance_id,
                    &playlist.external_id,
                    playlist.cover_url.clone(),
                    source.can_play_online,
                ));
            }
            if !ui.is_rect_visible(rect) {
                return;
            }
            let dark = ui.visuals().dark_mode;
            let fill = if response.hovered() || response.has_focus() {
                style::hovered(dark)
            } else {
                style::surface(dark)
            };
            let border = if response.has_focus() {
                style::accent(ui)
            } else if dark {
                style::choice_border(true)
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke.color
            };
            ui.painter().rect(
                rect,
                7,
                fill,
                Stroke::new(1_f32, border),
                StrokeKind::Inside,
            );
            let inner = rect.shrink(9.);
            let side = inner.height().min((inner.width() * 0.40).max(1.));
            let cover = Rect::from_min_size(
                egui::pos2(inner.left(), inner.center().y - side / 2.),
                egui::Vec2::splat(side),
            );
            let count = playlist
                .track_count
                .map(|n| tr("{0} 首").replace("{0}", &n.to_string()));
            let metadata = [playlist.creator.as_deref(), count.as_deref()]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            paint_cover(ui, cover, playlist.cover_url.as_deref());
            let x = cover.right() + 10.;
            let width = (inner.right() - x).max(1.);
            let text = |s: &str, font: FontId, color: Color32, rows| {
                let mut job = egui::text::LayoutJob::simple(s.into(), font, color, width);
                job.wrap.max_rows = rows;
                ui.fonts(|fonts| fonts.layout_job(job))
            };
            let primary = style::text(dark);
            let secondary = style::muted(dark);
            let title_text = text(title, style::bold(ui, 15.), primary, 2);
            let details = [&metadata[..], item.reason.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(|s| text(s, FontId::proportional(12.), secondary, 1))
                .collect::<Vec<_>>();
            let height = title_text.size().y + details.iter().map(|g| g.size().y + 3.).sum::<f32>();
            let mut pos = egui::pos2(x, inner.center().y - height / 2.);
            let painter = ui.painter().with_clip_rect(inner.intersect(ui.clip_rect()));
            let title_height = title_text.size().y;
            painter.galley(pos, title_text, primary);
            pos.y += title_height + 3.;
            for line in details {
                let line_height = line.size().y;
                painter.galley(pos, line, secondary);
                pos.y += line_height + 3.;
            }
            response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(title);
        },
    );
    selected
}
