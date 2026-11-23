#[cfg(test)]
mod interaction_tests;
mod layout;
mod row;
mod widgets;

use super::{
    actions::Actions,
    catalog::Catalog,
    data::{play_row, queue_track, CategoryTab, Services, TrackQuery},
    style,
};
use crate::i18n::tr;
use eframe::egui;
use layout::{Columns, Line, Lines, HEADER, HEIGHT};
use musicfs_player_core::Status;
use musicfs_server_adapter::Playlist;
use row::{Action, RowEvent};

fn key(track: &musicfs_server_adapter::Track) -> (&str, Option<&str>) {
    (&track.id, track.forced_source_track_id.as_deref())
}

fn time(ms: Option<u64>) -> String {
    ms.map(|ms| format!("{:02}:{:02}", ms / 60000, ms / 1000 % 60))
        .unwrap_or_else(|| "—".into())
}

pub fn tracks(
    ui: &mut egui::Ui,
    svc: Services<'_>,
    data: &mut Catalog,
    actions: &mut Actions,
    query: TrackQuery,
    lists: &[Playlist],
    active_source: Option<&str>,
    status: Status,
) {
    let screen = ui.ctx().screen_rect();
    let right_margin = if screen.width() <= 850. { 18. } else { 30. };
    let right = screen.right() - right_margin;
    let table_width = (right - ui.next_widget_position().x)
        .min(ui.available_width())
        .max(0.);
    data.sync(ui, svc, &query);
    data.artwork.poll(ui.ctx());
    data.providers.sync(ui, svc);
    if data.table_toolbar(ui, svc, actions, &query, lists) {
        return;
    }
    if data.versions.poll(ui, svc, actions) {
        data.reset_tracks();
        return;
    }
    if let Some(error) = data.tracks.error.as_ref() {
        ui.colored_label(ui.visuals().error_fg_color, error);
        if ui.button(tr("重试加载曲目")).clicked() {
            data.more(ui, svc);
        }
    }
    if data.tracks.items.is_empty() {
        if !data.tracks.busy() && data.tracks.error.is_none() {
            if data.tracks.next.is_some() {
                data.more(ui, svc);
                return;
            }
            let label = match &query {
                TrackQuery::Library(q)
                | TrackQuery::AvailableSources { query: q, .. }
                | TrackQuery::Category { query: q, .. }
                    if !q.is_empty() =>
                {
                    tr("没有找到相关曲目")
                }
                TrackQuery::Library(_) | TrackQuery::AvailableSources { .. } => {
                    tr("在服务端添加音乐目录并扫描后，曲目会出现在这里。")
                }
                TrackQuery::Favorites => tr("收藏列表为空"),
                TrackQuery::History => tr("暂无播放记录"),
                TrackQuery::Playlist { .. } => tr("歌单中暂无曲目"),
                TrackQuery::Category { .. } => tr("此分类暂无曲目"),
            };
            ui.vertical_centered(|ui| {
                ui.add_space(70.);
                ui.weak(label);
            });
        }
        return;
    }
    let library = matches!(
        query,
        TrackQuery::AvailableSources { .. } | TrackQuery::Category { .. }
    );
    let can_match = library || matches!(query, TrackQuery::Library(_));
    let favorites = query == TrackQuery::Favorites;
    let lines = Lines::new(
        data.tracks.items.len(),
        data.versions.expanded(&data.tracks.items),
        data.tracks.next.is_some(),
    );
    let mut event = None;
    let mut more = false;
    let dark = ui.visuals().dark_mode;
    let stroke = egui::Stroke::new(1_f32, style::line(dark));
    egui::Frame::new()
        .stroke(stroke)
        .corner_radius(9)
        .inner_margin(0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let width = (table_width - 2.).min(ui.available_width()).max(0.);
            ui.set_width(width);
            let cols = Columns::new(width, !library);
            let (hdr, _) = ui.allocate_exact_size(egui::vec2(width, HEADER), egui::Sense::hover());
            widgets::header(ui, hdr, &cols, data);
            ui.painter().hline(hdr.x_range(), hdr.bottom(), stroke);
            egui::ScrollArea::vertical()
                .id_salt("track-data")
                .auto_shrink([false, true])
                .show_rows(ui, HEIGHT, lines.len, |ui, range| {
                    for line in range {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(width.min(ui.available_width()), HEIGHT),
                            egui::Sense::hover(),
                        );
                        let display = lines.at(line);
                        let mut cell = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt(("track-slot", line))
                                .max_rect(rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        match display {
                            Line::Track(i) => {
                                if let Some(result) = row::show(
                                    &mut cell,
                                    rect,
                                    &cols,
                                    data,
                                    actions,
                                    svc,
                                    i,
                                    favorites,
                                    library,
                                    active_source,
                                    status,
                                ) {
                                    event = Some((i, result));
                                }
                            }
                            Line::Source(i, n) => {
                                ui.painter().rect_filled(rect, 0, style::surface(dark));
                                data.versions.source(
                                    &mut cell,
                                    svc,
                                    &data.tracks.items,
                                    i,
                                    n,
                                    &data.providers,
                                    can_match.then_some(&mut data.matching),
                                    actions,
                                    lists,
                                );
                            }
                            Line::More => {
                                cell.add_space(14.);
                                more = cell
                                    .add_enabled(
                                        !data.tracks.busy(),
                                        egui::Button::new(tr("加载更多曲目")),
                                    )
                                    .clicked();
                            }
                        }
                        if line + 1 < lines.len {
                            ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
                        }
                    }
                });
        });
    if let Some((i, event)) = event {
        apply(ui, svc, data, actions, lists, i, event, favorites);
    }
    if more {
        data.more(ui, svc);
    }
}

fn apply(
    ui: &mut egui::Ui,
    svc: Services<'_>,
    data: &mut Catalog,
    actions: &mut Actions,
    lists: &[Playlist],
    i: usize,
    event: RowEvent,
    favorites: bool,
) {
    let (action, multi) = match event {
        RowEvent::Credit(tab, id, name) => {
            data.open_credit(tab, id, name);
            return;
        }
        RowEvent::Action(action, multi) => (action, multi),
    };
    let track = &data.tracks.items[i];
    let ordered;
    let selected = if multi {
        let mut keys: std::collections::HashSet<_> =
            data.matching.selected_tracks().iter().map(key).collect();
        ordered = data
            .tracks
            .items
            .iter()
            .filter(|t| keys.remove(&key(t)))
            .cloned()
            .collect::<Vec<_>>();
        ordered.as_slice()
    } else {
        std::slice::from_ref(track)
    };
    let queue = || selected.iter().filter_map(queue_track);
    let result = match action {
        Action::Play if multi => svc.player.set_queue(queue().collect(), 0).map(|_| ()),
        Action::Play => play_row(svc.player, &data.tracks.items, i),
        Action::Next if multi => svc.player.play_next_many(queue().collect()),
        Action::Next => queue_track(track).map_or(Ok(()), |t| svc.player.play_next(t)),
        Action::Queue => svc.player.enqueue_many(queue().collect()).map(|_| ()),
        Action::Favorite if multi => {
            actions.favorite_many(ui, svc, selected);
            Ok(())
        }
        Action::Favorite => {
            if let Some(on) = actions.favorites.contains(&track.id, favorites) {
                actions.toggle(ui, svc, track, !on);
            }
            Ok(())
        }
        Action::Add => {
            if multi {
                actions.open_many(selected, lists)
            } else {
                actions.open(track, lists)
            };
            Ok(())
        }
        Action::Versions => {
            data.versions.toggle(ui, svc, i, track);
            Ok(())
        }
    };
    if let Err(e) = result {
        actions.notice = Some((e, true));
    }
}
