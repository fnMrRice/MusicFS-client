use eframe::egui::{self, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, Vec2};

pub const ACCENT: Color32 = Color32::from_rgb(255, 54, 88);

pub fn background(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(17, 22, 27)
    } else {
        Color32::from_rgb(247, 248, 250)
    }
}

pub fn sidebar(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(16, 20, 25)
    } else {
        Color32::from_rgb(238, 240, 243)
    }
}

pub fn surface(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(25, 31, 37)
    } else {
        Color32::WHITE
    }
}

pub fn muted(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(151, 162, 174)
    } else {
        Color32::from_rgb(96, 109, 122)
    }
}

pub fn apply(ctx: &egui::Context, value: &str) {
    use egui::{Theme, ThemePreference};
    ctx.set_theme(match value {
        "light" => ThemePreference::Light,
        "system" => ThemePreference::System,
        _ => ThemePreference::Dark,
    });
    for theme in [Theme::Dark, Theme::Light] {
        let dark = theme == Theme::Dark;
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = Vec2::new(8.0, 7.0);
            s.spacing.button_padding = Vec2::new(11.0, 7.0);
            s.spacing.interact_size.y = 31.0;
            s.text_styles
                .insert(egui::TextStyle::Body, FontId::proportional(14.0));
            s.text_styles
                .insert(egui::TextStyle::Button, FontId::proportional(14.0));
            s.text_styles
                .insert(egui::TextStyle::Heading, FontId::proportional(24.0));
            s.visuals.panel_fill = background(dark);
            s.visuals.window_fill = surface(dark);
            s.visuals.extreme_bg_color = if dark {
                Color32::from_rgb(28, 35, 43)
            } else {
                Color32::WHITE
            };
            s.visuals.widgets.noninteractive.fg_stroke.color = if dark {
                Color32::from_rgb(235, 239, 244)
            } else {
                Color32::from_rgb(39, 51, 65)
            };
            s.visuals.widgets.noninteractive.bg_stroke.color = if dark {
                Color32::from_rgb(39, 46, 53)
            } else {
                Color32::from_rgb(215, 221, 227)
            };
            s.visuals.widgets.inactive.weak_bg_fill = surface(dark);
            s.visuals.widgets.hovered.weak_bg_fill = if dark {
                Color32::from_rgb(48, 55, 62)
            } else {
                Color32::from_rgb(219, 225, 232)
            };
            s.visuals.selection.bg_fill = if dark {
                Color32::from_rgb(107, 38, 56)
            } else {
                Color32::from_rgb(255, 197, 207)
            };
            s.visuals.hyperlink_color = if dark {
                Color32::from_rgb(255, 131, 151)
            } else {
                Color32::from_rgb(175, 25, 57)
            };
            s.visuals.striped = true;
        });
    }
}

pub fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
            .fill(ACCENT)
            .corner_radius(7)
            .min_size(Vec2::new(86.0, 34.0)),
    )
}

pub fn note(ui: &mut egui::Ui) {
    let dark = ui.visuals().dark_mode;
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(29.0), Sense::hover());
        let p = ui.painter();
        let pt = |x: f32, y: f32| Pos2::new(r.left() + x, r.top() + y);
        let stroke = Stroke::new(2.6_f32, ACCENT);
        p.line_segment([pt(9.0, 22.0), pt(9.0, 6.0)], stroke);
        p.line_segment([pt(9.0, 6.0), pt(25.0, 3.0)], stroke);
        p.line_segment([pt(9.0, 10.0), pt(25.0, 7.0)], stroke);
        p.line_segment([pt(25.0, 3.0), pt(25.0, 19.0)], stroke);
        p.circle_stroke(pt(5.5, 23.0), 3.5, stroke);
        p.circle_stroke(pt(21.5, 20.0), 3.5, stroke);
        ui.label(RichText::new("MusicFS").size(29.0).strong().color(if dark {
            Color32::WHITE
        } else {
            Color32::from_rgb(36, 46, 59)
        }));
    });
}

pub fn vinyl(ui: &mut egui::Ui, r: Rect) {
    let p = ui.painter_at(r);
    let width = r.width();
    let height = r.height();
    let center = r.center() + Vec2::new(width * 0.10, -height * 0.04);
    let radius = (width * 0.32).min(height * 0.37);
    p.rect_filled(r, 0, Color32::from_rgb(56, 39, 55));
    for i in 0..48 {
        let t = i as f32 / 48.0;
        p.rect_filled(
            Rect::from_min_max(
                Pos2::new(r.left(), r.top() + height * t),
                Pos2::new(r.right(), r.top() + height * (t + 1.0 / 48.0) + 1.0),
            ),
            0,
            Color32::from_rgb(
                (95.0 - t * 68.0) as u8,
                (63.0 - t * 37.0) as u8,
                (78.0 - t * 41.0) as u8,
            ),
        );
    }
    p.circle_filled(
        center + Vec2::splat(4.0),
        radius,
        Color32::from_black_alpha(90),
    );
    p.circle_filled(center, radius, Color32::from_rgb(26, 25, 32));
    for ring in 0..(radius as usize / 3) {
        p.circle_stroke(
            center,
            radius - ring as f32 * 3.0,
            Stroke::new(0.8_f32, Color32::from_rgb(56, 51, 61)),
        );
    }
    p.circle_filled(center, radius * 0.31, Color32::from_rgb(198, 91, 112));
    p.circle_filled(center, radius * 0.055, Color32::from_rgb(48, 35, 44));
    let size = radius * 1.55;
    let sleeve = Rect::from_center_size(
        center + Vec2::new(-size * 0.48, size * 0.22),
        Vec2::splat(size),
    );
    let jacket = p.with_clip_rect(sleeve.intersect(r));
    jacket.rect_filled(sleeve, 0, Color32::from_rgb(224, 157, 152));
    for (n, color) in [
        Color32::from_rgb(107, 53, 73),
        Color32::from_rgb(157, 81, 98),
        Color32::from_rgb(207, 121, 132),
    ]
    .into_iter()
    .enumerate()
    {
        jacket.circle_filled(
            sleeve.left_top() + Vec2::new(size * (0.72 - n as f32 * 0.15), -size * 0.13),
            size * 0.84,
            color,
        );
    }
}
