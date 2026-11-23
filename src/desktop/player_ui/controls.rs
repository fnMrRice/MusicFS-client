use crate::{icons, ui::style};
use eframe::egui::{self, Response, RichText, Ui};

pub(super) fn play_button(ui: &mut Ui, playing: bool, enabled: bool, size: f32) -> Response {
    let label = crate::i18n::tr(if playing { "暂停" } else { "播放" });
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, r) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
            let fill = style::accent_fill(ui);
            ui.painter().circle_filled(rect.center(), size / 2., fill);
            icons::paint(
                ui,
                if playing { "pause" } else { "play" },
                egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(size * 0.46)),
                style::on_accent(ui),
            );
            r
        })
        .inner;
    response.on_hover_text(label)
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
