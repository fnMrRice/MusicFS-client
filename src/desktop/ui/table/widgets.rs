use super::{layout::Columns, row::RowEvent, *};
use crate::icons;
use egui::{pos2, vec2, Align, Color32, Layout, Rect, Response, RichText, Sense, UiBuilder, Vec2};
use musicfs_player_core::Status;
use musicfs_server_adapter::TrackCredit;

pub(super) fn favorite_button(ui: &mut egui::Ui, saved: bool, label: &str) -> Response {
    crate::ui::favorites::favorite_button(ui, saved, label, 38.)
}

pub(super) fn contents(ui: &mut egui::Ui, rect: Rect, id: &str) -> egui::Ui {
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing.x = 0.;
    child
}

pub(super) fn label(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &str,
    text: &str,
    color: Color32,
    size: f32,
) {
    if rect.width() <= 0. {
        return;
    }
    let rect = rect.shrink2(vec2(8., 0.));
    contents(ui, rect, id)
        .add(egui::Label::new(RichText::new(text).size(size).color(color)).truncate())
        .on_hover_text(text);
}

pub(super) fn header(
    ui: &mut egui::Ui,
    rect: Rect,
    c: &Columns,
    checkbox: impl FnOnce(&mut egui::Ui, Rect),
) {
    let dark = ui.visuals().dark_mode;
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius {
            nw: 9,
            ne: 9,
            sw: 0,
            se: 0,
        },
        style::surface(dark),
    );
    let cells = c.cells(rect);
    if c.check > 0. {
        checkbox(ui, check_rect(cells[0]));
    }
    for (idx, name) in [
        (1, "#"),
        (2, "曲名"),
        (3, "歌手"),
        (4, "专辑"),
        (5, "来源"),
        (6, "格式"),
        (7, "时长"),
    ] {
        let mut r = cells[idx];
        if idx == 2 {
            r.min.x += 6.;
        }
        label(ui, r, name, tr(name), style::muted(dark), 13.);
    }
}

pub(super) fn select_all(ui: &mut egui::Ui, rect: Rect, data: &mut Catalog) {
    let selected: std::collections::HashSet<_> =
        data.matching.selected_tracks().iter().map(key).collect();
    let count = if selected.is_empty() {
        0
    } else {
        data.tracks
            .items
            .iter()
            .filter(|t| selected.contains(&key(t)))
            .count()
    };
    let mut all = count > 0 && count == data.tracks.items.len();
    let mut check = contents(ui, rect, "select-all");
    let partial = count > 0 && !all;
    style::checkbox_style(&mut check);
    let enabled = !data.matching.locked() && !data.tracks.items.is_empty();
    let response = style::add_checkbox(
        &mut check,
        enabled,
        all || partial,
        egui::Checkbox::without_text(&mut all).indeterminate(partial),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            enabled,
            all,
            tr("全选已加载曲目"),
        )
    });
    if response.on_hover_text(tr("全选已加载曲目")).changed() {
        data.matching.select_rows(&data.tracks.items, all);
    }
}

pub(super) fn check_rect(mut rect: Rect) -> Rect {
    rect.min.x += (rect.width() - 20.).min(16.);
    rect
}

pub(super) fn credit(
    ui: &mut egui::Ui,
    rect: Rect,
    credits: &[TrackCredit],
    fallback: &str,
    links: bool,
    tab: CategoryTab,
) -> Option<RowEvent> {
    if rect.width() < 1. {
        return None;
    }
    if !links || credits.is_empty() {
        label(
            ui,
            rect,
            tab.label(),
            fallback,
            ui.visuals().text_color(),
            13.,
        );
        return None;
    }
    let mut child = contents(ui, rect.shrink2(vec2(8., 0.)), tab.label());
    let full = credits
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(" / ");
    let font = egui::FontId::proportional(13.);
    let natural = child
        .painter()
        .layout_no_wrap(full.clone(), font, child.visuals().text_color())
        .size()
        .x;
    let mut chosen = None;
    if credits.len() > 1 && natural > child.available_width() {
        let button = style::add_button(
            &mut child,
            true,
            egui::Button::new(RichText::new(&full).size(13.).underline())
                .frame(false)
                .truncate(),
        )
        .on_hover_text(&full);
        let id = button.id.with("credits");
        if button.clicked() {
            child.memory_mut(|m| m.toggle_popup(id));
        }
        egui::popup_below_widget(
            &child,
            id,
            &button,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                for c in credits {
                    if style::add_button(ui, !c.id.is_empty(), egui::Button::new(&c.name)).clicked()
                    {
                        chosen = Some(c);
                    }
                }
            },
        );
    } else {
        child.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        for (j, c) in credits.iter().enumerate() {
            if j > 0 {
                child.label(" / ");
            }
            let text = RichText::new(&c.name).size(13.);
            if c.id.is_empty() {
                child.add(egui::Label::new(text).truncate());
            } else if style::link(&mut child, text.underline())
                .on_hover_text(&c.name)
                .clicked()
            {
                chosen = Some(c);
            }
        }
    }
    chosen.map(|c| RowEvent::Credit(tab, c.id.clone(), c.name.clone()))
}

pub(super) fn playback_indicator(ui: &mut egui::Ui, r: Rect, status: Status, number: usize) {
    let title = match status {
        Status::Playing => tr("正在播放"),
        Status::Paused => tr("已暂停"),
        Status::Loading => tr("正在加载…"),
        _ => "",
    };
    if title.is_empty() {
        label(
            ui,
            r,
            "row-number",
            &number.to_string(),
            style::muted(ui.visuals().dark_mode),
            12.,
        );
        return;
    }
    let center = r.center();
    let response = ui.interact(r, ui.id().with("playback-indicator"), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, title));
    response.on_hover_text(title);
    if status == Status::Paused {
        icons::paint(
            ui,
            "pause",
            Rect::from_center_size(center, Vec2::splat(14.)),
            style::accent(ui),
        );
        return;
    }
    let t = ui.input(|i| i.time) as f32;
    for (i, f) in [4.1, 5.4, 6.7].into_iter().enumerate() {
        let phase = (t * f + i as f32 * 1.8).sin();
        let h = if status == Status::Playing {
            7.5 + 4.5 * phase
        } else {
            3.
        };
        let x = center.x - 7. + i as f32 * 5.;
        let y = if status == Status::Playing {
            center.y + 6. - h
        } else {
            center.y - 1.5 + phase * 2.
        };
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(x, y), vec2(3., h)),
            1,
            style::accent(ui),
        );
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(80));
}

pub(super) enum TitleControl {
    Play { enabled: bool, current: bool },
    Versions { open: bool },
}

pub(super) fn title_control(
    ui: &mut egui::Ui,
    rect: Rect,
    control: TitleControl,
    text: &str,
    accessible: &str,
) -> Response {
    let dark = ui.visuals().dark_mode;
    let (icon, enabled, selected, size, base) = match control {
        TitleControl::Play { enabled, current } => {
            ("play", enabled, current, 14., style::text(dark))
        }
        TitleControl::Versions { open } => (
            if open { "chevron-up" } else { "chevron" },
            true,
            open,
            12.,
            style::muted(dark),
        ),
    };
    let id = if matches!(control, TitleControl::Play { .. }) {
        "play"
    } else {
        "versions"
    };
    let mut child = contents(ui, rect, id);
    if !enabled {
        child.disable();
    }
    let hit = child.interact(rect, child.id().with("button"), Sense::click());
    hit.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, accessible));
    let motion = style::interaction(&child, &hit);
    let color = if !enabled {
        style::muted(dark)
    } else if selected {
        child.painter().rect_filled(
            rect,
            4.,
            motion.color(
                Color32::TRANSPARENT,
                style::hovered(dark),
                style::accent_soft(ui),
            ),
        );
        style::highlight(dark)
    } else {
        motion.color(base, style::accent(ui), style::text(dark))
    };
    let center = pos2(rect.left() + 7., rect.center().y);
    icons::paint(
        ui,
        icon,
        Rect::from_center_size(center, Vec2::splat(14. - motion.press)),
        color,
    );
    let mut text_rect = rect;
    text_rect.min.x += 19.;
    contents(&mut child, text_rect, "title-text").add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .truncate()
            .selectable(false),
    );
    hit.on_hover_text(accessible)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
