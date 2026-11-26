use crate::i18n::tr;
use eframe::egui;
use musicfs_player_core::{PlayerCommand, PlayerSnapshot};

pub(super) fn show(ui: &mut egui::Ui, snapshot: &PlayerSnapshot) -> Option<PlayerCommand> {
    let mut command = None;
    ui.horizontal(|ui| {
        ui.weak(tr("{0} 首曲目").replace("{0}", &snapshot.queue.len().to_string()));
        if ui
            .add_enabled(
                !snapshot.queue.is_empty(),
                egui::Button::new(tr("清空队列")),
            )
            .clicked()
        {
            command = Some(PlayerCommand::Clear);
        }
    });
    ui.separator();
    if snapshot.queue.is_empty() {
        ui.weak(tr("队列为空"));
        return command;
    }

    ui.spacing_mut().scroll.floating = false;
    let row_height =
        ui.spacing().interact_size.y.max(
            ui.text_style_height(&egui::TextStyle::Button) + 2. * ui.spacing().button_padding.y,
        );
    egui::ScrollArea::vertical()
        .id_salt("queue-tracks")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, snapshot.queue.len(), |ui, range| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            for index in range {
                let track = &snapshot.queue[index];
                let label = if track.artists.is_empty() {
                    track.title.clone()
                } else {
                    format!("{}   ·   {}", track.title, track.artists.join(" / "))
                };
                ui.horizontal(|ui| {
                    ui.weak(format!("{:02}", index + 1));
                    if ui
                        .selectable_label(snapshot.index == Some(index), &label)
                        .on_hover_text(label)
                        .clicked()
                    {
                        command = Some(PlayerCommand::Select { index });
                    }
                });
            }
        });
    command
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
