use crate::{i18n::tr, icons, ui::style};
use eframe::egui::{self, Response, RichText, Ui};
use musicfs_player_core::PlaybackMode;

#[cfg(test)]
#[path = "../../../tests/native/playback_mode_menu.rs"]
mod tests;

pub(crate) const PLAYBACK_MODES: [(PlaybackMode, &str); 4] = [
    (PlaybackMode::Sequence, "顺序播放"),
    (PlaybackMode::RepeatOne, "单曲循环"),
    (PlaybackMode::Shuffle, "随机循环"),
    (PlaybackMode::RepeatAll, "列表循环"),
];

pub(super) fn playback_mode_menu(ui: &mut Ui, current: PlaybackMode) -> Option<PlaybackMode> {
    let icon = match current {
        PlaybackMode::Sequence => "sequence",
        PlaybackMode::RepeatOne => "repeat-one",
        PlaybackMode::Shuffle => "shuffle",
        PlaybackMode::RepeatAll => "repeat",
    };
    let (_, label) = PLAYBACK_MODES.iter().find(|(mode, _)| *mode == current)?;
    let hint = tr("播放顺序：{0}").replace("{0}", tr(label));
    let id = ui.next_auto_id().with("playback-order");
    let open = ui.memory(|m| m.is_popup_open(id));
    let button = icons::selectable_button(ui, icon, &hint, 36., 21., open);
    if button.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    let mut changed = None;
    egui::popup_above_or_below_widget(
        ui,
        id,
        &button,
        egui::AboveOrBelow::Above,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(136.);
            let mut selected = current;
            for (mode, label) in PLAYBACK_MODES {
                let option = style::radio_value(ui, &mut selected, mode, tr(label));
                if button.clicked() && button.has_focus() && mode == current {
                    option.request_focus();
                }
                if option.clicked() {
                    if mode != current {
                        changed = Some(mode);
                    }
                    ui.memory_mut(|m| m.close_popup());
                    button.request_focus();
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                button.request_focus();
            }
        },
    );
    changed
}

pub(super) fn play_button(ui: &mut Ui, playing: bool, enabled: bool, size: f32) -> Response {
    let label = crate::i18n::tr(if playing { "暂停" } else { "播放" });
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, r) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
            let motion = style::interaction(ui, &r);
            let diameter = size * (1. - 0.06 * motion.press);
            ui.painter()
                .circle_filled(rect.center(), diameter / 2., motion.primary_fill(ui));
            icons::paint(
                ui,
                if playing { "pause" } else { "play" },
                egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(diameter * 0.46)),
                style::ON_ACCENT,
            );
            r
        })
        .inner;
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

pub(super) fn heading(ui: &mut Ui, title: &str, artists: &[String]) {
    ui.add(egui::Label::new(RichText::new(title).size(16.).strong()).truncate());
    let name = if artists.is_empty() {
        crate::i18n::tr("未知歌手").into()
    } else {
        artists.join(" / ")
    };
    ui.add(egui::Label::new(RichText::new(name).weak().size(12.)).truncate());
}
