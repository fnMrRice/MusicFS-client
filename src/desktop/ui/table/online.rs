use super::{
    layout::Columns,
    time, view,
    widgets::{header, label, title_control, TitleControl},
};
use crate::{i18n::tr, ui::style};
use eframe::egui;
use musicfs_server_adapter::RecommendedPlaylistDetail;

pub(crate) fn online_tracks(
    ui: &mut egui::Ui,
    scroll: egui::ScrollArea,
    detail: &RecommendedPlaylistDetail,
    order: &[usize],
    source_name: &str,
    can_play: bool,
    has_more: bool,
    mut paint_cover: impl FnMut(&egui::Ui, egui::Rect, Option<&str>),
) -> (Option<usize>, bool) {
    let mut play = None;
    let mut more_visible = false;
    let width = view::width(ui);
    view::show(
        ui,
        scroll,
        width,
        Columns::online,
        order.len() + usize::from(has_more),
        |ui, rect, columns, slot| {
            let Some(slot) = slot else {
                header(ui, rect, columns, |_, _| {});
                return;
            };
            let Some(&index) = order.get(slot) else {
                more_visible = ui.is_rect_visible(rect);
                return;
            };
            let entry = &detail.entries[index];
            let track = entry.track.as_ref();
            let playable = can_play && !entry.unavailable && track.is_some();
            let row = view::row_background(ui, rect, false);
            let [_, number, title, artist, album, source, _, duration, _] = columns.cells(rect);
            let fg = ui.visuals().text_color();
            let muted = style::muted(ui.visuals().dark_mode);
            label(
                ui,
                number,
                "number",
                &entry.position.saturating_add(1).to_string(),
                muted,
                12.,
            );

            let (cover, mut name_rect, mut subtitle_rect) = view::title_rects(title);
            paint_cover(ui, cover, track.and_then(|t| t.cover_url.as_deref()));
            let track_title = track.map(|t| t.title.as_str()).filter(|s| !s.is_empty());
            let artists = track.map(|t| t.artists.join(" / ")).unwrap_or_default();
            let album_name = track.and_then(|t| t.album.as_deref()).unwrap_or("—");
            let compact_detail = if columns.artist == 0. && !artists.is_empty() {
                artists.as_str()
            } else if columns.provider == 0. {
                source_name
            } else {
                ""
            };
            let (name, subtitle) = match (track_title, entry.unavailable) {
                (None, true) => (tr("曲目已失效"), entry.external_track_id.as_str()),
                (Some(title), true) => (title, tr("曲目已失效")),
                (title, false) => (title.unwrap_or(&entry.external_track_id), compact_detail),
            };
            if subtitle.is_empty() {
                name_rect =
                    name_rect.translate(egui::vec2(0., rect.center().y - name_rect.center().y));
            } else {
                subtitle_rect.min.x += 11.;
                label(ui, subtitle_rect, "subtitle", subtitle, muted, 11.);
            }
            let button = title_control(
                ui,
                name_rect,
                TitleControl::Play {
                    enabled: playable,
                    current: false,
                },
                name,
                &tr("播放 {0}").replace("{0}", name),
            );
            if playable && (button.clicked() || row.double_clicked()) {
                play = Some(index);
            }
            label(
                ui,
                artist,
                "artist",
                if artists.is_empty() { "—" } else { &artists },
                fg,
                13.,
            );
            label(ui, album, "album", album_name, fg, 13.);
            label(ui, source, "provider", source_name, fg, 13.);
            label(
                ui,
                duration,
                "duration",
                &time(track.and_then(|t| t.duration_ms)),
                muted,
                13.,
            );
        },
    );
    (play, more_visible)
}
