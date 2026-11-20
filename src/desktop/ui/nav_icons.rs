use eframe::egui::{self, Color32, Painter, Pos2, Stroke};

pub fn draw(p: &Painter, c: Pos2, icon: &str, color: Color32) {
    let at = |x, y| c + egui::vec2(x, y);
    let pen = Stroke::new(1.45_f32, color);
    let line = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            pts.iter().map(|&(x, y)| at(x, y)).collect(),
            pen,
        ));
    };
    match icon {
        "⌕" => {
            p.circle_stroke(at(-1.5, -1.5), 5.6, pen);
            line(&[(2.8, 2.8), (7.5, 7.5)]);
        }
        "⌂" => {
            line(&[
                (-8., -0.5),
                (0., -7.8),
                (8., -0.5),
                (7., -0.5),
                (7., 7.),
                (-7., 7.),
                (-7., -0.5),
            ]);
        }
        "♫" => {
            line(&[(-2., 4.), (-2., -5.), (7., -7.), (7., 2.)]);
            p.circle_stroke(at(-4.2, 4.6), 2.2, pen);
            p.circle_stroke(at(4.8, 2.6), 2.2, pen);
        }
        "▾" | "▸" => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(15., 15.)),
                2.,
                pen,
                egui::StrokeKind::Inside,
            );
            p.circle_stroke(c, 4., pen);
            p.circle_filled(c, 1., color);
        }
        "◉" | "disc" => {
            p.circle_stroke(c, 7., pen);
            p.circle_stroke(c, 1.8, pen);
            line(&[(-5., -1.), (-4., -3.), (-2., -4.5)]);
        }
        "headphones" => {
            line(&[
                (-7., 3.),
                (-7., -2.),
                (-5., -5.5),
                (-2., -7.),
                (2., -7.),
                (5., -5.5),
                (7., -2.),
                (7., 3.),
            ]);
            for x in [-5.5, 5.5] {
                p.rect_stroke(
                    egui::Rect::from_center_size(at(x, 4.), egui::vec2(3., 6.)),
                    1.,
                    pen,
                    egui::StrokeKind::Inside,
                );
            }
        }
        "radio" => {
            p.rect_stroke(
                egui::Rect::from_center_size(at(0., 2.), egui::vec2(15., 10.)),
                1.5,
                pen,
                egui::StrokeKind::Inside,
            );
            line(&[(-6., -3.), (5., -7.)]);
            p.circle_stroke(at(-3.5, 2.), 1.7, pen);
            line(&[(1., 1.), (5., 1.)]);
        }
        "♡" => line(&[
            (0., 7.),
            (-6.6, 0.2),
            (-7., -2.5),
            (-5.2, -5.2),
            (-2.2, -5.6),
            (0., -3.5),
            (2.2, -5.6),
            (5.2, -5.2),
            (7., -2.5),
            (6.6, 0.2),
            (0., 7.),
        ]),
        "◷" => {
            p.circle_stroke(c, 7., pen);
            line(&[(0., -4.5), (0., 0.), (3.5, 2.)]);
        }
        _ => {
            line(&[
                (-7., -5.5),
                (-2., -5.5),
                (0., -3.),
                (7., -3.),
                (7., 6.),
                (-7., 6.),
                (-7., -5.5),
            ]);
        }
    }
}
