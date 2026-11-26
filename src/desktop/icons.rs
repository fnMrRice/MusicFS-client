use crate::ui::style;
use eframe::egui::{
    self, Color32, ColorImage, Id, Rect, Response, Sense, TextureHandle, TextureOptions, Ui, Vec2,
    WidgetInfo, WidgetType,
};

// 名称与 controls.svg 中从左到右、从上到下的原画顺序一致。
const NAMES: &[&str] = &[
    "music",
    "search",
    "home",
    "folder",
    "calendar",
    "heart",
    "clock",
    "settings",
    "server",
    "theme",
    "back",
    "next",
    "previous",
    "play",
    "pause",
    "stop",
    "grid",
    "queue",
    "repeat",
    "repeat-one",
    "sequence",
    "shuffle",
    "volume",
    "volume-muted",
    "plus",
    "more",
    "chevron",
    "refresh",
    "user",
    "close",
    "pin",
    "album",
    "disc",
    "headphones",
    "radio",
    "lyrics",
    "heart-filled",
    "mini",
    "brand",
];
const COLUMNS: usize = 8;

fn atlas(ui: &Ui) -> TextureHandle {
    let id = Id::new("musicfs-control-atlas");
    if let Some(texture) = ui.ctx().data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let rgba = image::load_from_memory(include_bytes!("../app/icons/controls.png"))
        .expect("Invalid application icon atlas")
        .into_rgba8();
    let image = ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );
    let texture = ui
        .ctx()
        .load_texture("MusicFS controls", image, TextureOptions::LINEAR);
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

pub fn paint(ui: &Ui, name: &str, rect: Rect, tint: Color32) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let flip = name == "chevron-up";
    let icon = if flip { "chevron" } else { name };
    let index = NAMES.iter().position(|item| *item == icon).unwrap_or(0);
    let rows = NAMES.len().div_ceil(COLUMNS) as f32;
    let x = (index % COLUMNS) as f32;
    let y = (index / COLUMNS) as f32;
    let mut uv = Rect::from_min_max(
        egui::pos2(x / COLUMNS as f32, y / rows),
        egui::pos2((x + 1.) / COLUMNS as f32, (y + 1.) / rows),
    );
    if flip {
        std::mem::swap(&mut uv.min.y, &mut uv.max.y);
    }
    ui.painter().image(atlas(ui).id(), rect, uv, tint);
}

pub fn button(ui: &mut Ui, name: &str, label: &str, target: f32, glyph: f32) -> Response {
    icon_button(ui, name, label, target, glyph, None)
}

pub fn selectable_button(
    ui: &mut Ui,
    name: &str,
    label: &str,
    target: f32,
    glyph: f32,
    selected: bool,
) -> Response {
    icon_button(ui, name, label, target, glyph, Some(selected))
}

fn icon_button(
    ui: &mut Ui,
    name: &str,
    label: &str,
    target: f32,
    glyph: f32,
    selected: Option<bool>,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(target), Sense::click());
    response.widget_info(|| match selected {
        Some(selected) => WidgetInfo::selected(
            WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        ),
        None => WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label),
    });
    let motion = style::interaction(ui, &response);
    let dark = ui.visuals().dark_mode;
    let selection = selected.map_or(0., |value| {
        ui.ctx()
            .animate_bool(response.id.with("icon-selected"), value)
    });
    let tint = ui.visuals().selection.bg_fill;
    let fill = motion.color(
        Color32::TRANSPARENT.lerp_to_gamma(style::accent_soft(ui), selection),
        style::hovered(dark).lerp_to_gamma(tint, selection),
        tint.lerp_to_gamma(style::accent_fill(ui), selection * 0.22),
    );
    let ink = style::text(dark).lerp_to_gamma(style::highlight(dark), selection);
    ui.painter().rect_filled(rect.shrink(motion.press), 7, fill);
    paint(
        ui,
        name,
        Rect::from_center_size(
            rect.center(),
            Vec2::splat(glyph * (1. - 0.06 * motion.press)),
        ),
        ink,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}
