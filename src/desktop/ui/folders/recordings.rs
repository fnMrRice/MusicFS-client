use super::{cards::line, playable, Providers};
use crate::{i18n::tr, icons, ui::style};
use eframe::egui::{
    self, pos2, vec2, Align, Color32, FontId, Layout, Rect, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, WidgetInfo, WidgetType,
};
use musicfs_server_adapter::folders::Recording;

#[cfg(test)]
#[path = "../../../../tests/native/folder_recording_rows.rs"]
mod tests;

pub(super) enum Action {
    Play(usize),
    Enqueue(usize),
}

const ROW: f32 = 54.;

struct Cells {
    title: Rect,
    artist: Rect,
    provider: Rect,
    duration: Rect,
    queue: Rect,
}

impl Cells {
    fn new(rect: Rect) -> Self {
        let width = rect.width();
        let action = (width * 0.15).min(43.);
        let time = if width >= 290. { 60. } else { 0. };
        let artist = if width >= 540. { width * 0.21 } else { 0. };
        let provider = if width >= 730. { width * 0.15 } else { 0. };
        let indent = if width >= 185. { 47. } else { 8. };
        let mut right = rect.right();
        let mut cell = |w: f32| {
            let cell = Rect::from_min_max(pos2(right - w, rect.top()), pos2(right, rect.bottom()));
            right -= w;
            cell
        };
        let queue = cell(action);
        let duration = cell(time);
        let provider = cell(provider);
        let artist = cell(artist);
        let title = Rect::from_min_max(
            pos2((rect.left() + indent).min(right), rect.top()),
            pos2(right, rect.bottom()),
        );
        Self {
            title,
            artist,
            provider,
            duration,
            queue,
        }
    }
}

fn field(ui: &Ui, mut cell: Rect, text: &str, dy: f32, font: FontId, color: Color32) {
    if cell.width() > 8. {
        cell.max.x -= 8.;
        line(ui, cell, dy, text, font, color);
    }
}

pub(super) fn list(ui: &mut Ui, tracks: &[Recording], providers: &Providers) -> Option<Action> {
    if tracks.is_empty() {
        return None;
    }
    let dark = ui.visuals().dark_mode;
    let border = Stroke::new(1.0_f32, style::line(dark));
    let mut action = None;
    egui::Frame::new()
        .stroke(border)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let width = ui.available_width();
            let (_, header) = ui.allocate_space(vec2(width, 32.));
            ui.painter().rect_filled(
                header,
                egui::CornerRadius {
                    nw: 8,
                    ne: 8,
                    ..Default::default()
                },
                style::surface(dark),
            );
            let head = Cells::new(header);
            for (cell, key) in [
                (head.title, "曲名"),
                (head.artist, "歌手"),
                (head.provider, "来源"),
                (head.duration, "时长"),
            ] {
                field(
                    ui,
                    cell,
                    tr(key),
                    0.,
                    FontId::proportional(12.),
                    style::muted(dark),
                );
            }
            ui.painter()
                .hline(header.x_range(), header.bottom(), border);
            // 目录和曲目共享外层滚动区，预留完整表高，只为可见行和相邻的键盘焦点建立控件。
            let (_, body) = ui.allocate_space(vec2(width, ROW * tracks.len() as f32));
            let first = ((ui.clip_rect().top() - body.top()) / ROW).floor() as usize;
            let end = ((ui.clip_rect().bottom() - body.top()) / ROW).ceil() as usize;
            for (index, track) in tracks
                .iter()
                .enumerate()
                .take(end.saturating_add(1))
                .skip(first.saturating_sub(1))
            {
                let rect =
                    Rect::from_min_size(body.min + vec2(0., ROW * index as f32), vec2(width, ROW));
                let mut row = ui.new_child(
                    UiBuilder::new()
                        .id_salt(&track.source_track_id)
                        .max_rect(rect),
                );
                if let Some(event) = recording_row(&mut row, rect, track, providers, index) {
                    action = Some(event);
                }
                if index + 1 < tracks.len() {
                    ui.painter().hline(rect.x_range(), rect.bottom(), border);
                }
            }
        });
    action
}

fn recording_row(
    ui: &mut Ui,
    rect: Rect,
    track: &Recording,
    providers: &Providers,
    index: usize,
) -> Option<Action> {
    let cells = Cells::new(rect);
    let title = track
        .title
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&track.display_name);
    let artists = track.artists.as_deref().unwrap_or_default().join(" / ");
    let provider = providers.label(&track.provider_instance_id);
    let duration = track
        .duration_ms
        .map(|ms| format!("{}:{:02}", ms / 60_000, ms / 1000 % 60));
    let available = playable(track);
    let mut detail = Vec::new();
    if !available {
        detail.push(tr("不可播放"));
    }
    if track.display_name != title && !track.display_name.is_empty() {
        detail.push(&track.display_name);
    }
    if cells.artist.width() == 0. && !artists.is_empty() {
        detail.push(&artists);
    }
    if cells.provider.width() == 0. {
        detail.push(provider);
    }
    if cells.duration.width() == 0. {
        detail.extend(duration.as_deref());
    }
    let detail = detail.join(" · ");
    let mut details = vec![title];
    if track.display_name != title && !track.display_name.is_empty() {
        details.push(&track.display_name);
    }
    if !artists.is_empty() {
        details.push(&artists);
    }
    details.push(provider);
    details.extend(duration.as_deref());
    if !available {
        details.push(tr("不可播放"));
    }
    let meta = details.join("\n");
    let dark = ui.visuals().dark_mode;
    let muted = style::muted(dark);
    let ink = if available { style::text(dark) } else { muted };
    let play_area = Rect::from_min_max(rect.min, cells.queue.left_bottom());
    let mut play_target = ui.new_child(UiBuilder::new().id_salt("play-target").max_rect(play_area));
    if !available {
        play_target.disable();
    }
    let play = play_target.interact(play_area, play_target.id().with("play"), Sense::click());
    let play_label = tr("播放 {0}").replace("{0}", title);
    play.widget_info(|| WidgetInfo::labeled(WidgetType::Button, available, &play_label));
    ui.ctx()
        .accesskit_node_builder(play.id, |node| node.set_description(meta.clone()));
    let motion = style::interaction(ui, &play);
    ui.painter().rect_filled(
        play_area,
        4,
        motion.color(
            Color32::TRANSPARENT,
            style::hovered(dark),
            style::accent_soft(ui),
        ),
    );
    if play.has_focus() {
        ui.painter().rect_stroke(
            play_area.shrink(1.),
            4,
            Stroke::new(1.0_f32, style::accent(ui)),
            StrokeKind::Inside,
        );
    }
    if cells.title.left() - rect.left() > 40. {
        let center = pos2(rect.left() + 24., rect.center().y);
        let circle = Rect::from_center_size(center, vec2(28., 28.));
        ui.painter().rect_filled(circle, 7, style::surface(dark));
        icons::paint(
            ui,
            if available { "play" } else { "music" },
            circle.shrink(7.),
            motion.color(muted, style::accent(ui), ink),
        );
    }
    field(
        ui,
        cells.title,
        title,
        if detail.is_empty() { 0. } else { -9. },
        FontId::proportional(14.),
        ink,
    );
    if !detail.is_empty() {
        field(
            ui,
            cells.title,
            &detail,
            10.,
            FontId::proportional(12.),
            muted,
        );
    }
    field(
        ui,
        cells.artist,
        if artists.is_empty() { "—" } else { &artists },
        0.,
        FontId::proportional(13.),
        muted,
    );
    field(
        ui,
        cells.provider,
        provider,
        0.,
        FontId::proportional(13.),
        muted,
    );
    field(
        ui,
        cells.duration,
        duration.as_deref().unwrap_or("—"),
        0.,
        FontId::proportional(13.),
        muted,
    );
    if play.gained_focus() {
        play.scroll_to_me(None);
    }
    if !available {
        play.on_disabled_hover_text(meta);
        return None;
    }
    let play = play
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(meta)
        .clicked();
    let mut queue = ui.new_child(
        UiBuilder::new()
            .id_salt("enqueue")
            .max_rect(cells.queue)
            .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
    );
    let label = format!("{} · {title}", tr("加入队列"));
    let target = cells.queue.width().min(32.);
    let add = icons::button(&mut queue, "plus", &label, target, target.min(16.));
    if add.gained_focus() {
        add.scroll_to_me(Some(Align::Center));
    }
    if add.clicked() {
        Some(Action::Enqueue(index))
    } else if play {
        Some(Action::Play(index))
    } else {
        None
    }
}
