use eframe::egui::{
    self, Color32, Pos2, Response, RichText, Sense, Shape, Stroke, Ui, WidgetInfo, WidgetType,
};

pub const ACCENT: Color32 = Color32::from_rgb(255, 55, 88);

#[derive(Clone, Copy, PartialEq)]
pub enum Glyph {
    Previous,
    Play,
    Pause,
    Next,
    Volume,
    Muted,
    Queue,
    Mini,
    Lyrics,
    Record,
}

pub fn glyph(ui: &Ui, center: Pos2, icon: Glyph, ink: Color32, scale: f32) {
    let point = |x, y| center + egui::vec2(x, y) * scale;
    let pen = Stroke::new(1.65 * scale, ink);
    let line = |coords: &[(f32, f32)]| {
        ui.painter().add(Shape::line(
            coords.iter().map(|&(x, y)| point(x, y)).collect(),
            pen,
        ));
    };
    let triangle = |a: Pos2, b: Pos2, c: Pos2| {
        ui.painter()
            .add(Shape::convex_polygon(vec![a, b, c], ink, Stroke::NONE));
    };
    match icon {
        Glyph::Play => triangle(point(-4., -7.), point(7., 0.), point(-4., 7.)),
        Glyph::Pause => {
            for x in [-4.0, 3.0] {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(point(x - 1.5, -7.), point(x + 1.5, 7.)),
                    1.0,
                    ink,
                );
            }
        }
        Glyph::Next | Glyph::Previous => {
            let sign = if icon == Glyph::Next { 1.0 } else { -1.0 };
            triangle(
                point(-6. * sign, -6.),
                point(4. * sign, 0.),
                point(-6. * sign, 6.),
            );
            line(&[(6. * sign, -6.), (6. * sign, 6.)]);
        }
        Glyph::Volume | Glyph::Muted => {
            line(&[
                (-8., -3.),
                (-5., -3.),
                (-1., -7.),
                (-1., 7.),
                (-5., 3.),
                (-8., 3.),
                (-8., -3.),
            ]);
            if icon == Glyph::Muted {
                line(&[(3., -3.), (8., 3.)]);
                line(&[(8., -3.), (3., 3.)]);
            } else {
                line(&[(3., -4.), (5., 0.), (3., 4.)]);
                line(&[(6., -7.), (9., 0.), (6., 7.)]);
            }
        }
        Glyph::Queue => {
            for y in [-5., 0., 5.] {
                line(&[(-3., y), (8., y)]);
                ui.painter().circle_filled(point(-7., y), scale, ink);
            }
        }
        Glyph::Mini => {
            line(&[(-8., -6.), (8., -6.), (8., 6.), (-8., 6.), (-8., -6.)]);
            ui.painter().rect_filled(
                egui::Rect::from_min_max(point(0., 0.), point(6., 4.)),
                1.,
                ink,
            );
        }
        Glyph::Lyrics => {
            line(&[
                (-8., -6.),
                (8., -6.),
                (8., 4.),
                (-2., 4.),
                (-6., 8.),
                (-6., 4.),
                (-8., 4.),
                (-8., -6.),
            ]);
            line(&[(-5., -2.), (5., -2.)]);
            line(&[(-5., 1.), (2., 1.)]);
        }
        Glyph::Record => {
            ui.painter().circle_stroke(center, 9. * scale, pen);
            ui.painter().circle_stroke(center, 2.3 * scale, pen);
            line(&[(-6., -2.), (-4.8, -4.8), (-2., -6.)]);
            line(&[(6., 2.), (4.8, 4.8), (2., 6.)]);
        }
    }
}

pub fn icon_button(
    ui: &mut Ui,
    icon: Glyph,
    label: &str,
    enabled: bool,
    primary: bool,
) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        let size = if primary { 39. } else { 30. };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
        let ink = if enabled {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().weak_text_color().gamma_multiply(0.60)
        };
        if primary {
            ui.painter().circle_filled(
                rect.center(),
                size / 2.0,
                if enabled {
                    ACCENT
                } else {
                    ACCENT.gamma_multiply(0.45)
                },
            );
        } else if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, 6.0, ui.visuals().widgets.hovered.bg_fill);
        }
        glyph(
            ui,
            rect.center(),
            icon,
            if primary { Color32::WHITE } else { ink },
            1.,
        );
        response.on_hover_text(label)
    })
    .inner
}

pub fn heading(ui: &mut Ui, title: &str, artists: &[String]) {
    ui.add(egui::Label::new(RichText::new(title).size(16.).strong()).truncate());
    let name = if artists.is_empty() {
        "—".to_owned()
    } else {
        artists.join(" / ")
    };
    ui.add(egui::Label::new(RichText::new(name).weak().size(12.)).truncate());
}
