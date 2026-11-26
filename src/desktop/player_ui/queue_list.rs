use super::lyrics::time;
use crate::{i18n::tr, icons, ui::style};
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui};
use musicfs_player_core::{PlayerCommand, PlayerSnapshot, Status, Track};

const ROW_HEIGHT: f32 = 47.;

pub(super) fn show(ui: &mut Ui, snapshot: &PlayerSnapshot) -> Option<PlayerCommand> {
    let scroll_key = egui::Id::new("musicfs-queue-scroll-position");
    if snapshot.queue.is_empty() {
        ui.data_mut(|d| d.remove::<(egui::Id, f32)>(scroll_key));
        let rect = ui.available_rect_before_wrap();
        let dark = ui.visuals().dark_mode;
        let center = rect.center() - egui::vec2(0., 14.);
        let tile = Rect::from_center_size(center, egui::Vec2::splat(42.));
        ui.painter()
            .circle_filled(center, 21., style::accent_soft(ui));
        icons::paint(ui, "queue", tile.shrink(11.), style::accent(ui));
        let label = tr("队列为空");
        let response = ui.allocate_rect(rect, Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
        ui.painter().text(
            center + egui::vec2(0., 32.),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.),
            style::muted(dark),
        );
        return None;
    }

    ui.spacing_mut().scroll.floating = false;
    ui.spacing_mut().item_spacing.y = 1.;
    let id = ui.make_persistent_id(egui::Id::new("queue-tracks"));
    let saved = ui.data(|d| d.get_temp::<(egui::Id, f32)>(scroll_key));
    let mut area = egui::ScrollArea::vertical()
        .id_salt("queue-tracks")
        .auto_shrink([false, false]);
    // 弹层和独立窗口的控件 ID 不同，切换时从同一个队列阅读位置继续。
    if saved.is_none_or(|(last_id, _)| last_id != id) {
        area = area.vertical_scroll_offset(saved.map_or(0., |(_, y)| y));
    }
    let number_width = (snapshot.queue.len().to_string().len() as f32 * 8. + 12.).max(34.);
    let mut command = None;
    let area = area.show_rows(ui, ROW_HEIGHT, snapshot.queue.len(), |ui, range| {
        for index in range {
            let current = snapshot.index == Some(index);
            let row = track_row(
                ui,
                &snapshot.queue[index],
                index,
                current,
                snapshot.status,
                number_width,
            );
            if row.clicked() {
                command = Some(PlayerCommand::Select { index });
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(scroll_key, (id, area.state.offset.y)));
    command
}

fn track_row(
    ui: &mut Ui,
    track: &Track,
    index: usize,
    current: bool,
    status: Status,
    number_width: f32,
) -> egui::Response {
    let (full, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let artists = track.artists.join(" / ");
    let label = if artists.is_empty() {
        track.title.clone()
    } else {
        format!("{} · {artists}", track.title)
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, current, &label)
    });
    let dark = ui.visuals().dark_mode;
    let row = full.shrink2(egui::vec2(6., 0.));
    let accent = style::accent(ui);
    let muted = style::muted(dark);
    let painter = ui.painter();
    let motion = style::interaction(ui, &response);
    let tint = style::accent_soft(ui);
    let fill = if current {
        let accent_fill = style::accent_fill(ui);
        motion.color(
            tint,
            tint.lerp_to_gamma(accent_fill, 0.12),
            tint.lerp_to_gamma(accent_fill, 0.24),
        )
    } else {
        motion.color(Color32::TRANSPARENT, style::hovered(dark), tint)
    };
    painter.rect_filled(row, 6, fill);
    if response.has_focus() {
        painter.rect_stroke(row, 6, Stroke::new(1_f32, accent), StrokeKind::Inside);
    }
    let index_center = egui::pos2(row.left() + 3. + number_width * 0.5, row.center().y);
    if current {
        if status == Status::Playing {
            for (n, height) in [7., 14., 10.].into_iter().enumerate() {
                let foot = index_center + egui::vec2((n as f32 - 1.) * 4., 7.);
                painter.rect_filled(
                    Rect::from_min_size(foot - egui::vec2(1.25, height), egui::vec2(2.5, height)),
                    1.,
                    accent,
                );
            }
        } else {
            icons::paint(
                ui,
                if status == Status::Paused {
                    "pause"
                } else {
                    "music"
                },
                Rect::from_center_size(index_center, egui::Vec2::splat(15.)),
                accent,
            );
        }
    } else {
        painter.text(
            index_center,
            Align2::CENTER_CENTER,
            (index + 1).to_string(),
            FontId::proportional(12.),
            muted,
        );
    }
    let left = row.left() + number_width + 7.;
    let right = if let Some(duration) = track.duration_ms {
        let time = painter.text(
            egui::pos2(row.right() - 9., row.center().y),
            Align2::RIGHT_CENTER,
            time(duration),
            FontId::proportional(12.),
            muted,
        );
        time.left() - 12.
    } else {
        row.right() - 9.
    };
    let title_color = if current {
        style::highlight(dark)
    } else {
        style::text(dark)
    };
    let title_y = row.center().y - if artists.is_empty() { 0. } else { 8. };
    let title_font = if current {
        style::bold(ui, 14.)
    } else {
        FontId::proportional(14.)
    };
    text_line(
        ui,
        &track.title,
        egui::pos2(left, title_y),
        right,
        title_font,
        title_color,
    );
    if !artists.is_empty() {
        text_line(
            ui,
            &artists,
            egui::pos2(left, row.center().y + 10.),
            right,
            FontId::proportional(12.),
            muted,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

fn text_line(
    ui: &Ui,
    text: &str,
    center_left: egui::Pos2,
    right: f32,
    font: FontId,
    color: Color32,
) {
    let width = (right - center_left.x).max(1.);
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, width);
    job.wrap.max_rows = 1;
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let pos = center_left - egui::vec2(0., galley.size().y / 2.);
    ui.painter().galley(pos, galley, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_long_titles_keep_the_scrollbar_at_the_right_edge() {
        for title in ["A".to_owned(), "Long track title ".repeat(80)] {
            let track: musicfs_player_core::Track = serde_json::from_value(serde_json::json!({
                "id": "track",
                "title": title,
                "artists": ["Artist"]
            }))
            .unwrap();
            let snapshot = PlayerSnapshot {
                queue: vec![track; 60].into(),
                ..Default::default()
            };
            let ctx = egui::Context::default();
            ctx.style_mut(|s| s.animation_time = 0.);
            let frame = |events| {
                let mut offset = 0.;
                let mut edge = egui::Pos2::ZERO;
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300., 240.),
                    )),
                    events,
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        edge = bounds.right_bottom() - egui::vec2(2., 8.);
                        show(ui, &snapshot);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.5);
                        let id = ui.make_persistent_id(egui::Id::new("queue-tracks"));
                        offset = egui::scroll_area::State::load(ctx, id).unwrap().offset.y;
                    });
                });
                (offset, edge)
            };
            frame(vec![]);
            let (top, edge) = frame(vec![]);
            assert_eq!(top, 0.);
            let (scrolled, _) = frame(vec![
                egui::Event::PointerMoved(edge),
                egui::Event::PointerButton {
                    pos: edge,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            assert!(scrolled > 0.);
        }
    }
}
