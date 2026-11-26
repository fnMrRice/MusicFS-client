use crate::ui::style::{self, ComboBoxMotion};
use eframe::egui::{self, Color32, Event, Modifiers, PointerButton, Pos2, Rect, Shape};

const LABEL: &str = "Sample";

#[derive(Clone, Copy, Debug)]
enum Control {
    Button,
    Combo,
    Primary { enabled: bool },
    Choice { selected: bool },
}

struct Frame {
    rect: Rect,
    fill: Color32,
    ink: Color32,
    clicked: bool,
}

struct Scene {
    ctx: egui::Context,
    time: f64,
}

impl Scene {
    fn new(theme: &str) -> Self {
        let ctx = egui::Context::default();
        style::apply(&ctx, theme, "default");
        Self { ctx, time: 0. }
    }

    fn frame(&mut self, control: Control, events: Vec<Event>) -> Frame {
        self.time += 1. / 120.;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(320., 120.))),
            time: Some(self.time),
            predicted_dt: 1. / 120.,
            events,
            ..Default::default()
        };
        let mut rect = Rect::NOTHING;
        let mut clicked = false;
        let output = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let response = match control {
                    Control::Button => style::button(ui, LABEL),
                    Control::Combo => {
                        egui::ComboBox::from_id_salt("motion-test")
                            .selected_text(LABEL)
                            .show_animated(ui, |_| ())
                            .response
                    }
                    Control::Primary { enabled } => style::primary(ui, LABEL, enabled),
                    Control::Choice { selected } => style::selectable(ui, selected, LABEL),
                };
                rect = response.rect;
                clicked = response.clicked();
            });
        });
        let mut fill = Color32::TRANSPARENT;
        let mut ink = None;
        for clipped in output.shapes {
            match clipped.shape {
                Shape::Rect(shape) if shape.rect.center().distance(rect.center()) <= 1. => {
                    if shape.fill != Color32::TRANSPARENT {
                        fill = shape.fill;
                    }
                }
                Shape::Text(shape) if shape.galley.text() == LABEL => {
                    let color = shape.galley.job.sections[0].format.color;
                    ink = Some(shape.override_text_color.unwrap_or(
                        if color == Color32::PLACEHOLDER {
                            shape.fallback_color
                        } else {
                            color
                        },
                    ));
                }
                _ => {}
            }
        }
        Frame {
            rect,
            fill,
            ink: ink.expect("button label must be painted"),
            clicked,
        }
    }

    // 逐帧读取真正绘制的填色，同时涵盖渐变的起始帧与稳定后的颜色。
    fn transition(&mut self, control: Control, event: Event) -> Vec<Color32> {
        let mut colors = vec![self.frame(control, vec![event]).fill];
        colors.extend((0..48).map(|_| self.frame(control, vec![]).fill));
        colors
    }
}

fn pointer(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        pressed,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
    }
}

fn assert_fades(start: Color32, colors: &[Color32]) -> Color32 {
    let end = *colors.last().unwrap();
    assert_ne!(start, end, "interaction must change the painted fill");
    assert!(
        colors.iter().any(|&c| c != start && c != end),
        "the fill must pass through an intermediate color"
    );
    end
}

#[test]
fn hover_press_and_release_paint_intermediate_colors_without_moving_hit_targets() {
    for control in [Control::Button, Control::Primary { enabled: true }] {
        let mut scene = Scene::new("dark");
        let idle = scene.frame(control, vec![]);
        let pos = idle.rect.center();
        let hovered = assert_fades(
            idle.fill,
            &scene.transition(control, Event::PointerMoved(pos)),
        );
        let pressed = assert_fades(hovered, &scene.transition(control, pointer(pos, true)));
        let released = assert_fades(pressed, &scene.transition(control, pointer(pos, false)));
        assert_eq!(released, hovered, "release should settle back to hover");
        assert_eq!(scene.frame(control, vec![]).rect, idle.rect);
    }
}

#[test]
fn leaving_a_button_or_combo_fades_back_to_idle() {
    for control in [
        Control::Button,
        Control::Combo,
        Control::Choice { selected: true },
    ] {
        let mut scene = Scene::new("light");
        let idle = scene.frame(control, vec![]);
        let hovered = assert_fades(
            idle.fill,
            &scene.transition(control, Event::PointerMoved(idle.rect.center())),
        );
        let settled = assert_fades(hovered, &scene.transition(control, Event::PointerGone));
        assert_eq!(settled, idle.fill);
    }
}

#[test]
fn choices_use_theme_highlights_and_return_to_regular_text_when_deselected() {
    for (theme, dark, highlight) in [
        ("dark", true, Color32::WHITE),
        ("light", false, Color32::BLACK),
    ] {
        let mut scene = Scene::new(theme);
        let off = Control::Choice { selected: false };
        let on = Control::Choice { selected: true };
        let ordinary = scene.frame(off, vec![]).ink;
        assert_eq!(ordinary, style::text(dark));
        assert_ne!(ordinary, highlight);
        for _ in 0..48 {
            scene.frame(on, vec![]);
        }
        assert_eq!(scene.frame(on, vec![]).ink, highlight);
        for _ in 0..48 {
            scene.frame(off, vec![]);
        }
        assert_eq!(scene.frame(off, vec![]).ink, ordinary);
    }
}

#[test]
fn disabled_primary_ignores_hover_and_click() {
    let mut scene = Scene::new("dark");
    let control = Control::Primary { enabled: false };
    let idle = scene.frame(control, vec![]);
    let pos = idle.rect.center();
    for event in [
        Event::PointerMoved(pos),
        pointer(pos, true),
        pointer(pos, false),
    ] {
        let frame = scene.frame(control, vec![event]);
        assert!(!frame.clicked);
        assert_eq!(frame.fill, idle.fill);
    }
}
