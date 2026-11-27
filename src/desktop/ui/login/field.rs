use super::style;
use crate::{i18n::tr, icons};
use eframe::egui::{
    self, pos2, vec2, Align, Margin, Rect, Response, RichText, Stroke, StrokeKind, Ui,
};

#[derive(Clone, Copy)]
pub(super) enum Field {
    Endpoint,
    Token,
}

impl Field {
    pub(super) fn show(self, ui: &mut Ui, value: &mut String, enabled: bool) -> Response {
        let (id, label, hint) = match self {
            Self::Endpoint => ("login-endpoint", tr("服务器地址"), "http://127.0.0.1:8080"),
            Self::Token => ("login-token", tr("管理员登录令牌"), tr("输入登录令牌")),
        };
        let label = ui.label(RichText::new(label).font(style::bold(ui, 13.)));
        ui.add_enabled_ui(enabled, |ui| {
            let dark = ui.visuals().dark_mode;
            let accent = style::accent(ui);
            let muted = style::muted(dark);
            let visuals = ui.visuals_mut();
            visuals.widgets.inactive.bg_stroke = Stroke::new(1_f32, style::choice_border(dark));
            visuals.widgets.hovered.bg_stroke = Stroke::new(1_f32, muted);
            visuals.selection.stroke = Stroke::new(2_f32, accent);
            visuals.text_cursor.stroke.color = accent;

            let margin = Margin {
                left: 42,
                right: 14,
                top: 6,
                bottom: 6,
            };
            let response = ui
                .add(
                    egui::TextEdit::singleline(value)
                        .id_salt(id)
                        .password(matches!(self, Self::Token))
                        .hint_text(RichText::new(hint).color(muted))
                        .margin(margin)
                        .min_size(vec2(0., 44.))
                        .desired_width(f32::INFINITY)
                        .vertical_align(Align::Center),
                )
                .labelled_by(label.id);

            // egui 0.31 的 TextEdit 响应矩形不含内边距，图标绘于左侧留白。
            let outer = response.rect + margin;
            let center = pos2(outer.left() + 21., outer.center().y);
            let ink = if response.has_focus() { accent } else { muted };
            match self {
                Self::Endpoint => icons::paint(
                    ui,
                    "server",
                    Rect::from_center_size(center, vec2(18., 18.)),
                    ink,
                ),
                Self::Token => {
                    let stroke = Stroke::new(1.5_f32, ink);
                    let shackle = Rect::from_center_size(center - vec2(0., 3.), vec2(9., 10.));
                    let body = Rect::from_center_size(center + vec2(0., 3.), vec2(15., 11.));
                    ui.painter()
                        .rect_stroke(shackle, 4, stroke, StrokeKind::Inside);
                    ui.painter()
                        .rect(body, 3, style::input(dark), stroke, StrokeKind::Inside);
                    ui.painter().circle_filled(body.center(), 1.2, ink);
                }
            }
            response
        })
        .inner
    }
}
