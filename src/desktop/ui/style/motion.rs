use super::{accent, accent_fill, accent_soft, highlight, hovered, text, ON_ACCENT};
use eframe::egui::{
    self, Button, Checkbox, Color32, Id, Response, RichText, Sense, Stroke, Ui, Vec2, WidgetText,
};

#[derive(Clone, Copy, Default)]
pub struct Interaction {
    pub hover: f32,
    pub press: f32,
}

impl Interaction {
    pub fn color(self, rest: Color32, hovered: Color32, pressed: Color32) -> Color32 {
        rest.lerp_to_gamma(hovered, self.hover)
            .lerp_to_gamma(pressed, self.press)
    }

    pub fn primary_fill(self, ui: &Ui) -> Color32 {
        let base = accent_fill(ui);
        self.color(
            base,
            base.lerp_to_gamma(Color32::WHITE, 0.13),
            base.lerp_to_gamma(Color32::WHITE, 0.29),
        )
    }
}

fn animate(
    ui: &Ui,
    id: Id,
    enabled: bool,
    response: Option<&Response>,
    within: bool,
) -> Interaction {
    let response = response.filter(|r| enabled && ui.is_enabled() && r.enabled());
    let over = response.is_some_and(|r| within || r.hovered() || r.has_focus() || r.highlighted());
    let down = response.is_some_and(|r| r.is_pointer_button_down_on() || r.clicked());
    let ctx = ui.ctx();
    // 同一帧收到按下和松开时，也保留一次可见的点击反馈。
    if response.is_some_and(|r| r.clicked()) {
        ctx.request_repaint();
    }
    let hover = ctx.animate_bool_with_time(id.with("hover"), over, 0.14);
    let press = ctx.animate_bool_with_time(id.with("press"), down, if down { 0.065 } else { 0.12 });
    if enabled && ui.is_enabled() {
        Interaction { hover, press }
    } else {
        Interaction::default()
    }
}

pub fn interaction(ui: &Ui, response: &Response) -> Interaction {
    interaction_with_hover(ui, response, false)
}

pub fn interaction_with_hover(ui: &Ui, response: &Response, within: bool) -> Interaction {
    animate(ui, response.id, response.enabled(), Some(response), within)
}

fn prior_motion(ui: &Ui, id: Id, enabled: bool) -> Interaction {
    animate(ui, id, enabled, ui.ctx().read_response(id).as_ref(), false)
}

// egui 在每帧开始时完成命中测试，可在原生控件绘制前按其真实 ID 读取本帧交互。
pub(super) fn with_motion<R>(
    ui: &mut Ui,
    id: Id,
    enabled: bool,
    render: impl FnOnce(&mut Ui) -> R,
) -> R {
    let motion = prior_motion(ui, id, enabled);
    if motion.hover == 0. && motion.press == 0. {
        return render(ui);
    }
    let previous = ui.style().clone();
    let v = ui.visuals_mut();
    let w = &mut v.widgets;
    let (rest, over, down) = (w.inactive, w.hovered, w.active);
    let mut animated = rest;
    animated.bg_fill = motion.color(rest.bg_fill, over.bg_fill, down.bg_fill);
    animated.weak_bg_fill = motion.color(rest.weak_bg_fill, over.weak_bg_fill, down.weak_bg_fill);
    animated.bg_stroke.color = motion.color(
        rest.bg_stroke.color,
        over.bg_stroke.color,
        down.bg_stroke.color,
    );
    animated.fg_stroke.color =
        motion.color(rest.text_color(), over.text_color(), down.text_color());
    w.inactive = animated;
    w.hovered = animated;
    w.active = animated;
    v.hyperlink_color = motion.color(v.hyperlink_color, over.text_color(), down.bg_stroke.color);
    let result = render(ui);
    ui.set_style(previous);
    result
}

pub fn add_button(ui: &mut Ui, enabled: bool, button: Button<'_>) -> Response {
    with_motion(ui, ui.next_auto_id(), enabled, |ui| {
        ui.add_enabled(enabled, button)
    })
}

pub fn button(ui: &mut Ui, label: impl Into<WidgetText>) -> Response {
    add_button(ui, true, Button::new(label))
}

pub fn small_button(ui: &mut Ui, label: impl Into<WidgetText>) -> Response {
    add_button(ui, true, Button::new(label).small())
}

pub fn link(ui: &mut Ui, label: impl Into<WidgetText>) -> Response {
    let response = with_motion(ui, ui.next_auto_id(), true, |ui| ui.link(label));
    // egui 的文本选择会把链接重新标成普通标签，保留读屏与键盘操作所需的链接角色。
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Link)
    });
    response
}

pub fn menu_button<R>(
    ui: &mut Ui,
    label: impl Into<WidgetText>,
    contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    with_motion(ui, ui.next_auto_id(), true, |ui| {
        ui.menu_button(label, contents)
    })
}

fn with_selection(
    ui: &mut Ui,
    enabled: bool,
    selected: bool,
    render: impl FnOnce(&mut Ui) -> Response,
) -> Response {
    if !selected {
        return with_motion(ui, ui.next_auto_id(), enabled, render);
    }
    let previous = ui.style().clone();
    let ink = highlight(ui.visuals().dark_mode);
    let w = &mut ui.visuals_mut().widgets;
    for widget in [&mut w.inactive, &mut w.hovered, &mut w.active] {
        widget.fg_stroke.color = ink;
    }
    let result = with_motion(ui, ui.next_auto_id(), enabled, render);
    ui.set_style(previous);
    result
}

pub fn add_checkbox(
    ui: &mut Ui,
    enabled: bool,
    selected: bool,
    checkbox: Checkbox<'_>,
) -> Response {
    with_selection(ui, enabled, selected, |ui| {
        ui.add_enabled(enabled, checkbox)
    })
}

pub fn radio_value<T: PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    label: impl Into<WidgetText>,
) -> Response {
    with_selection(ui, true, *current == value, |ui| {
        ui.radio_value(current, value, label)
    })
}

pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: impl Into<WidgetText>) -> Response {
    add_checkbox(ui, true, *checked, Checkbox::new(checked, label))
}

pub fn primary_sized(ui: &mut Ui, label: &str, enabled: bool, size: Vec2) -> Response {
    let motion = prior_motion(ui, ui.next_auto_id(), enabled);
    let fill = motion.primary_fill(ui);
    ui.add_enabled(
        enabled,
        Button::new(RichText::new(label).color(ON_ACCENT))
            .min_size(size)
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(7),
    )
}

pub fn selectable(ui: &mut Ui, selected: bool, label: impl Into<WidgetText>) -> Response {
    let padding = ui.spacing().button_padding;
    let galley = label.into().into_galley(
        ui,
        None,
        ui.available_width() - 2. * padding.x,
        egui::TextStyle::Button,
    );
    let mut size = galley.size() + 2. * padding;
    size.y = size.y.max(ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_at_least(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            galley.text(),
        )
    });
    if ui.is_rect_visible(rect) {
        let dark = ui.visuals().dark_mode;
        let motion = interaction(ui, &response);
        let selection =
            ui.ctx()
                .animate_bool_with_time(response.id.with("selected"), selected, 0.14);
        let tint = ui.visuals().selection.bg_fill;
        let base = Color32::TRANSPARENT.lerp_to_gamma(tint, selection);
        let over =
            hovered(dark).lerp_to_gamma(tint.lerp_to_gamma(accent_fill(ui), 0.12), selection);
        let down =
            accent_soft(ui).lerp_to_gamma(tint.lerp_to_gamma(accent_fill(ui), 0.24), selection);
        let edge = if response.has_focus() {
            Stroke::new(1_f32, accent(ui))
        } else {
            Stroke::NONE
        };
        ui.painter().rect(
            rect,
            7,
            motion.color(base, over, down),
            edge,
            egui::StrokeKind::Inside,
        );
        let pos = ui
            .layout()
            .align_size_within_rect(galley.size(), rect.shrink2(padding))
            .min;
        ui.painter().galley(
            pos,
            galley,
            text(dark).lerp_to_gamma(highlight(dark), selection),
        );
    }
    response
}

pub fn selectable_value<T: PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    label: impl Into<WidgetText>,
) -> Response {
    let mut response = selectable(ui, *current == value, label);
    if response.clicked() && *current != value {
        *current = value;
        response.mark_changed();
    }
    response
}

#[cfg(test)]
#[path = "../../../../tests/native/button_motion.rs"]
mod tests;
