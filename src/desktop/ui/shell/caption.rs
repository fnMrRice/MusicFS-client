use super::style;
use crate::{i18n::tr, system};
use eframe::egui::{
    self, vec2, Align, Color32, Layout, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    ViewportCommand,
};

const BUTTON_WIDTH: f32 = 44.;
pub(super) const HEIGHT: f32 = 64.;
pub(super) const WIDTH: f32 = 3. * BUTTON_WIDTH;

pub(super) fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(1., 18.), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0_f32, style::choice_border(ui.visuals().dark_mode)),
    );
}

#[derive(Copy, Clone, PartialEq)]
enum Control {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Control {
    fn button(self, ui: &mut Ui, height: f32) -> Rect {
        let (rect, response) = ui.allocate_exact_size(vec2(BUTTON_WIDTH, height), Sense::click());
        let label = tr(match self {
            Self::Minimize => "最小化窗口",
            Self::Maximize => "最大化窗口",
            Self::Restore => "还原窗口",
            Self::Close => "关闭窗口",
        });
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
        let native_hover =
            matches!(self, Self::Maximize | Self::Restore) && system::maximize_hovered(ui.ctx());
        let motion = style::interaction_with_hover(ui, &response, native_hover);
        let dark = ui.visuals().dark_mode;
        let (hover, pressed, ink) = if self == Self::Close {
            let light = style::text(true);
            (
                Color32::from_rgb(196, 43, 52),
                Color32::from_rgb(165, 34, 43),
                motion.color(style::text(dark), light, light),
            )
        } else {
            (
                style::hovered(dark),
                style::accent_soft(ui),
                style::text(dark),
            )
        };
        ui.painter()
            .rect_filled(rect, 0., motion.color(Color32::TRANSPARENT, hover, pressed));
        let p = ui.painter();
        let c = rect.center();
        let stroke = Stroke::new(1.2_f32, ink);
        match self {
            Self::Minimize => {
                p.hline((c.x - 5.)..=(c.x + 5.), c.y, stroke);
            }
            Self::Maximize => {
                p.rect_stroke(
                    Rect::from_center_size(c, vec2(10., 10.)),
                    1,
                    stroke,
                    StrokeKind::Inside,
                );
            }
            Self::Restore => {
                let front = Rect::from_min_size(c + vec2(-5., -2.5), vec2(8., 8.));
                p.rect_stroke(front, 1, stroke, StrokeKind::Inside);
                p.add(egui::Shape::line(
                    vec![
                        front.left_top() + vec2(2.5, 0.),
                        front.left_top() + vec2(2.5, -2.5),
                        front.right_top() + vec2(2.5, -2.5),
                        front.right_bottom() + vec2(2.5, -2.5),
                        front.right_bottom() + vec2(0., -2.5),
                    ],
                    stroke,
                ));
            }
            Self::Close => {
                p.line_segment([c + vec2(-5., -5.), c + vec2(5., 5.)], stroke);
                p.line_segment([c + vec2(5., -5.), c + vec2(-5., 5.)], stroke);
            }
        }
        if response.on_hover_text(label).clicked() {
            ui.ctx().send_viewport_cmd(match self {
                Self::Minimize => ViewportCommand::Minimized(true),
                Self::Maximize => ViewportCommand::Maximized(true),
                Self::Restore => ViewportCommand::Maximized(false),
                Self::Close => ViewportCommand::Close,
            });
        }
        rect
    }
}

// 最大化按钮的实际矩形也用于 Windows 的贴靠布局命中测试。
pub(super) fn controls(ui: &mut Ui, header: Rect, drag: &[Rect]) {
    let mut strip = ui.new_child(
        UiBuilder::new()
            .max_rect(header.with_min_x(header.right() - WIDTH))
            .layout(Layout::left_to_right(Align::Center)),
    );
    strip.spacing_mut().item_spacing.x = 0.;
    Control::Minimize.button(&mut strip, header.height());
    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or_default());
    let maximize = if maximized {
        Control::Restore
    } else {
        Control::Maximize
    };
    let max_rect = maximize.button(&mut strip, header.height());
    Control::Close.button(&mut strip, header.height());
    system::publish_titlebar(ui.ctx(), drag, max_rect);
}

// 窗口控件叠在登录页背景上，返回下方可交互的表单区域。
pub(in crate::ui) fn login_title_bar(ui: &mut Ui) -> Rect {
    let bounds = ui.max_rect();
    let header = bounds.with_max_y(bounds.top() + HEIGHT);
    controls(ui, header, &[header.with_max_x(header.right() - WIDTH)]);
    bounds.with_min_y(header.bottom())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::{Action, ActionRequest, Role};

    #[test]
    fn caption_buttons_are_adjacent_and_send_root_window_commands() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        for maximized in [false, true] {
            let frame = |events| {
                let mut input = egui::RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(800., 500.))),
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .maximized = Some(maximized);
                ctx.run(input, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let form_area = login_title_bar(ui);
                            assert_eq!(form_area, ui.max_rect().with_min_y(HEIGHT));
                        });
                })
            };
            let tree = frame(vec![]).platform_output.accesskit_update.unwrap();
            let cases = [
                ("最小化窗口", ViewportCommand::Minimized(true)),
                (
                    if maximized {
                        "还原窗口"
                    } else {
                        "最大化窗口"
                    },
                    ViewportCommand::Maximized(!maximized),
                ),
                ("关闭窗口", ViewportCommand::Close),
            ];
            for (index, (label, command)) in cases.into_iter().enumerate() {
                let (id, node) = tree
                    .nodes
                    .iter()
                    .find(|(_, node)| {
                        node.role() == Role::Button && node.label() == Some(tr(label))
                    })
                    .expect("caption button");
                let bounds = node.bounds().unwrap();
                assert_eq!(
                    bounds.x0,
                    f64::from(800. - WIDTH + index as f32 * BUTTON_WIDTH)
                );
                assert_eq!(bounds.width(), f64::from(BUTTON_WIDTH));
                assert_eq!(bounds.y0, 0.);
                assert_eq!(bounds.y1, f64::from(HEIGHT));
                let output = frame(vec![egui::Event::AccessKitActionRequest(ActionRequest {
                    target: *id,
                    action: Action::Click,
                    data: None,
                })]);
                assert!(output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .contains(&command));
            }
        }
    }
}
