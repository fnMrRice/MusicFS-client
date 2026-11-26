use super::action_row;
use crate::{i18n::tr, ui::style};
use eframe::egui;

#[test]
fn source_action_columns_align_at_wide_and_narrow_widths() {
    let ctx = egui::Context::default();
    crate::fonts::install(&ctx);
    style::apply(&ctx, "dark", "default");
    let long_name = "An unusually long source name ".repeat(16);
    let directories = [
        ("Download", "886 首 · 17.7 GiB"),
        (long_name.as_str(), "123456 首 · 1023.9 GiB"),
    ];
    let scrapers = [("MusicBrainz", ""), ("网易云音乐", ""), (&long_name, "")];
    for width in [850., 340.] {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 720.));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for (items, actions) in [
                    (&directories[..], ["编辑", "删除", "刷新元数据"]),
                    (&scrapers[..], ["↑", "↓", "移除"]),
                ] {
                    let bounds = ui.available_rect_before_wrap();
                    let mut first_row = None;
                    for (index, &(name, stats)) in items.iter().enumerate() {
                        let mut buttons = Vec::new();
                        ui.push_id((actions[0], index), |ui| {
                            action_row(
                                ui,
                                |ui| {
                                    ui.add(egui::Label::new(name).truncate());
                                    if !stats.is_empty() {
                                        ui.weak(stats);
                                    }
                                },
                                |ui| {
                                    for action in actions {
                                        buttons.push(style::button(ui, tr(action)).rect);
                                    }
                                },
                            );
                        });
                        for button in &buttons {
                            assert!(button.left() >= bounds.left() - 1.);
                            assert!(button.right() <= bounds.right() + 1.);
                        }
                        let first = first_row.get_or_insert_with(|| buttons.clone());
                        for (button, reference) in buttons.iter().zip(first.iter()) {
                            assert!((button.left() - reference.left()).abs() <= 1.);
                            assert!((button.right() - reference.right()).abs() <= 1.);
                        }
                    }
                }
            });
        });
    }
}
