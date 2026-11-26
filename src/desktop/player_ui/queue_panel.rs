use super::queue_list;
use crate::{i18n::tr, icons, ui::style};
use eframe::egui::{
    self, Align, Align2, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder,
};
use musicfs_player_core::{PlayerCommand, PlayerSnapshot};

#[derive(Default)]
pub(super) struct PanelResponse {
    pub command: Option<PlayerCommand>,
    pub toggle_pin: bool,
    pub close: bool,
    pub start_drag: bool,
}

#[cfg(test)]
#[path = "../../../tests/native/queue_panel.rs"]
mod tests;

pub(super) fn show(ui: &mut Ui, snapshot: &PlayerSnapshot, pinned: bool) -> PanelResponse {
    let mut result = PanelResponse::default();
    ui.spacing_mut().item_spacing.y = 0.;
    let width = ui.available_width();
    let (header, _) = ui.allocate_exact_size(egui::vec2(width, 53.), Sense::hover());
    let dark = ui.visuals().dark_mode;
    let actions = header.shrink2(egui::vec2(8., 0.));
    let mut controls = ui.new_child(
        UiBuilder::new()
            .max_rect(actions)
            .layout(Layout::right_to_left(Align::Center)),
    );
    controls.spacing_mut().item_spacing.x = 4.;
    controls.spacing_mut().button_padding = egui::vec2(7., 4.);
    controls.spacing_mut().interact_size.y = 28.;
    let close = icons::button(&mut controls, "close", tr("关闭"), 28., 14.);
    result.close = close.clicked();
    let pin_label = tr(if pinned {
        "取消固定播放队列"
    } else {
        "固定播放队列"
    });
    let (pin_rect, pin) = controls.allocate_exact_size(egui::Vec2::splat(28.), Sense::click());
    pin.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, pin_label));
    let pin_color = if pinned {
        style::accent(ui)
    } else {
        style::text(dark)
    };
    if pinned || pin.hovered() || pin.has_focus() {
        let fill = if pinned {
            style::accent_soft(ui)
        } else {
            style::hovered(dark)
        };
        ui.painter().rect_filled(pin_rect, 6, fill);
    }
    icons::paint(
        ui,
        "pin",
        Rect::from_center_size(pin_rect.center(), egui::Vec2::splat(15.)),
        pin_color,
    );
    result.toggle_pin = pin
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(pin_label)
        .clicked();
    let clear = controls.add_enabled(
        !snapshot.queue.is_empty(),
        egui::Button::new(RichText::new(tr("清空队列")).size(12.)).frame(false),
    );
    if clear.clicked() {
        result.command = Some(PlayerCommand::Clear);
    }

    let title_rect = header
        .with_max_x(clear.rect.left())
        .shrink2(egui::vec2(12., 0.));
    let mut title = ui.new_child(
        UiBuilder::new()
            .max_rect(title_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    title.spacing_mut().item_spacing.x = 7.;
    title.label(
        RichText::new(tr("播放队列"))
            .font(style::bold(ui, 17.))
            .color(style::text(dark)),
    );
    let count = snapshot.queue.len().to_string();
    let count_color = style::muted(dark);
    let count_text =
        ui.painter()
            .layout_no_wrap(count.clone(), FontId::proportional(12.), count_color);
    let (badge, count_response) = title.allocate_exact_size(
        egui::vec2((count_text.size().x + 12.).max(23.), 21.),
        Sense::hover(),
    );
    let count_label = tr("{0} 首曲目").replace("{0}", &count);
    count_response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &count_label));
    ui.painter().rect_filled(badge, 8, style::hovered(dark));
    ui.painter().galley(
        Align2::CENTER_CENTER
            .anchor_size(badge.center(), count_text.size())
            .min,
        count_text,
        count_color,
    );

    // 标题和留白都可拖动；从按钮上按下鼠标始终保留该按钮的点击行为。
    result.start_drag = pinned
        && ui.rect_contains_pointer(header)
        && ui.input(|input| {
            input.pointer.button_pressed(egui::PointerButton::Primary)
                && input.pointer.interact_pos().is_some_and(|pos| {
                    ![clear.rect, pin_rect, close.rect]
                        .iter()
                        .any(|rect| rect.contains(pos))
                })
        });
    ui.painter().hline(
        header.x_range(),
        header.bottom(),
        Stroke::new(
            1_f32,
            if dark {
                style::hovered(true)
            } else {
                style::line(false)
            },
        ),
    );
    ui.add_space(4.);
    if let Some(command) = queue_list::show(ui, snapshot) {
        result.command = Some(command);
    }
    result
}
