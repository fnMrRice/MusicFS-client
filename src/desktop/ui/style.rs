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
        Color32::from_rgb(23, 29, 35)
    } else {
        Color32::WHITE
    }
}

pub fn muted(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(138, 148, 159)
    } else {
        Color32::from_rgb(105, 117, 129)
    }
}

pub fn text(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(237, 240, 242)
    } else {
        Color32::from_rgb(38, 49, 61)
    }
}
pub fn input(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(25, 31, 37)
    } else {
        Color32::WHITE
    }
}
pub fn line(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(30, 35, 40)
    } else {
        Color32::from_rgb(228, 231, 234)
    }
}
pub fn hovered(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(35, 42, 49)
    } else {
        Color32::from_rgb(229, 232, 237)
    }
}

const MIKU: Color32 = Color32::from_rgb(57, 197, 187);
const TIANYI: Color32 = Color32::from_rgb(102, 204, 255);

fn fill_for(key: &str) -> Color32 {
    match key {
        "miku" => MIKU,
        "tianyi" => TIANYI,
        _ => ACCENT,
    }
}
pub fn accent_fill(ui: &egui::Ui) -> Color32 {
    ui.ctx()
        .data(|d| d.get_temp(egui::Id::new("musicfs-accent")))
        .unwrap_or(ACCENT)
}
pub fn accent(ui: &egui::Ui) -> Color32 {
    match (ui.visuals().dark_mode, accent_fill(ui)) {
        (false, MIKU) => Color32::from_rgb(0, 105, 96),
        (false, TIANYI) => Color32::from_rgb(7, 94, 138),
        (_, color) => color,
    }
}
pub fn on_accent(ui: &egui::Ui) -> Color32 {
    if accent_fill(ui) == ACCENT {
        Color32::WHITE
    } else {
        sidebar(true)
    }
}
pub fn accent_soft(ui: &egui::Ui) -> Color32 {
    // 按 Web 的 sRGB 色值混合。GPU 透明叠色在线性空间会让整行的底色过亮。
    blend(background(ui.visuals().dark_mode), accent_fill(ui), 0.09)
}
fn blend(base: Color32, over: Color32, opacity: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) * (1. - opacity) + f32::from(b) * opacity).round() as u8;
    Color32::from_rgb(
        mix(base.r(), over.r()),
        mix(base.g(), over.g()),
        mix(base.b(), over.b()),
    )
}

pub fn checkbox_style(ui: &mut egui::Ui) {
    let dark = ui.visuals().dark_mode;
    let accent = accent(ui);
    let widgets = &mut ui.visuals_mut().widgets;
    let edge = if dark {
        Color32::from_rgb(128, 127, 137)
    } else {
        Color32::from_rgb(141, 150, 158)
    };
    let fill = if dark {
        Color32::from_rgb(43, 42, 50)
    } else {
        Color32::WHITE
    };
    for widget in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        widget.corner_radius = 4.into();
        widget.bg_stroke = Stroke::new(1_f32, edge);
        widget.bg_fill = fill;
    }
    widgets.active.bg_stroke = Stroke::new(1_f32, accent);
}
pub fn bold(ui: &egui::Ui, size: f32) -> FontId {
    crate::fonts::bold(ui.ctx(), size)
}

pub fn apply(ctx: &egui::Context, value: &str, accent_key: &str) {
    use egui::{Theme, ThemePreference};
    let fill = fill_for(accent_key);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("musicfs-accent"), fill));
    ctx.set_theme(match value {
        "light" => ThemePreference::Light,
        "system" => ThemePreference::System,
        _ => ThemePreference::Dark,
    });
    let heading = crate::fonts::bold(ctx, 24.);
    for theme in [Theme::Dark, Theme::Light] {
        let dark = theme == Theme::Dark;
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = Vec2::new(8., 7.);
            s.spacing.button_padding = Vec2::new(14., 8.);
            s.spacing.interact_size.y = 34.;
            s.spacing.icon_width = 16.;
            s.spacing.scroll.bar_width = 6.;
            s.text_styles
                .insert(egui::TextStyle::Body, FontId::proportional(14.));
            s.text_styles
                .insert(egui::TextStyle::Button, FontId::proportional(13.));
            s.text_styles
                .insert(egui::TextStyle::Heading, heading.clone());
            let v = &mut s.visuals;
            let border = Stroke::new(1_f32, line(dark));
            v.panel_fill = background(dark);
            v.window_fill = surface(dark);
            v.window_stroke = border;
            v.window_corner_radius = 12.into();
            v.menu_corner_radius = 9.into();
            v.extreme_bg_color = input(dark);
            v.faint_bg_color = surface(dark);
            v.hyperlink_color = text(dark);
            v.selection.bg_fill = blend(background(dark), fill, 0.2);
            v.selection.stroke = Stroke::new(1_f32, text(dark));
            v.striped = false;
            let w = &mut v.widgets;
            for (widget, bg) in [
                (&mut w.noninteractive, background(dark)),
                (&mut w.inactive, surface(dark)),
                (&mut w.hovered, hovered(dark)),
                (&mut w.active, hovered(dark)),
                (&mut w.open, hovered(dark)),
            ] {
                widget.bg_fill = bg;
                widget.weak_bg_fill = bg;
                widget.bg_stroke = Stroke::new(
                    1_f32,
                    if dark {
                        Color32::from_rgb(36, 41, 47)
                    } else {
                        line(false)
                    },
                );
                widget.fg_stroke = Stroke::new(1_f32, text(dark));
                widget.corner_radius = 7.into();
                widget.expansion = 0.;
            }
            w.noninteractive.bg_stroke = border;
            w.active.bg_stroke = Stroke::new(1_f32, fill);
        });
    }
}

pub fn appearance_choice(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    swatch: Option<Color32>,
) -> Response {
    let response = if let Some(color) = swatch {
        let radius = 4.;
        let mut job = egui::text::LayoutJob::default();
        job.append(
            label,
            2. * radius + ui.spacing().item_spacing.x,
            egui::TextFormat::simple(
                egui::TextStyle::Button.resolve(ui.style()),
                Color32::PLACEHOLDER,
            ),
        );
        let response = ui.selectable_label(selected, job);
        let x = response.rect.left() + ui.spacing().button_padding.x + radius;
        ui.painter()
            .circle_filled(egui::pos2(x, response.rect.center().y), radius, color);
        response
    } else {
        ui.selectable_label(selected, label)
    };
    let dark = ui.visuals().dark_mode;
    let border = if selected && dark {
        blend(background(true), accent(ui), 0.65)
    } else if selected {
        accent(ui)
    } else if response.hovered() || response.has_focus() {
        if dark {
            blend(background(true), muted(true), 0.6)
        } else {
            muted(false)
        }
    } else {
        choice_border(dark)
    };
    let width: f32 = if response.has_focus() {
        2.
    } else if selected {
        1.5
    } else {
        1.
    };
    ui.painter().rect_stroke(
        response.rect,
        ui.style().interact(&response).corner_radius,
        Stroke::new(width, border),
        egui::StrokeKind::Inside,
    );
    response
}

pub fn choice_border(dark: bool) -> Color32 {
    blend(background(dark), muted(dark), if dark { 0.36 } else { 0.8 })
}

pub fn accent_choices(ui: &mut egui::Ui, current: &str) -> Option<String> {
    use crate::i18n::tr;
    let mut chosen = None;
    ui.label(
        RichText::new(tr("主题色"))
            .size(12.)
            .color(muted(ui.visuals().dark_mode)),
    );
    ui.horizontal_wrapped(|ui| {
        for (key, label) in [
            ("default", "默认"),
            ("miku", "初音绿"),
            ("tianyi", "天依蓝"),
        ] {
            if appearance_choice(ui, current == key, tr(label), Some(fill_for(key))).clicked() {
                chosen = Some(key.into());
            }
        }
    });
    chosen
}

pub fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).color(on_accent(ui)))
            .fill(accent_fill(ui))
            .stroke(Stroke::NONE)
            .corner_radius(7)
            .min_size(Vec2::new(82., 36.)),
    )
}

pub fn note(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.;
        let (r, _) = ui.allocate_exact_size(Vec2::splat(34.), Sense::hover());
        crate::icons::paint(ui, "brand", r, accent(ui));
        ui.label(
            RichText::new("MusicFS")
                .font(bold(ui, 26.))
                .color(text(ui.visuals().dark_mode)),
        );
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
