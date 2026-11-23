use super::{
    actions::Actions,
    catalog::Catalog,
    data::{play_row, Services, TrackQuery},
    style,
};
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Vec2};
use musicfs_server_adapter::Playlist;

fn time(ms: Option<u64>) -> String {
    match ms {
        Some(ms) => {
            let sec = ms / 1000;
            format!("{}:{:02}", sec / 60, sec % 60)
        }
        None => "—".into(),
    }
}

fn text_cell(ui: &mut egui::Ui, width: f32, text: &str, color: Color32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 28.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_width(width);
            ui.add(egui::Label::new(RichText::new(text).color(color)).truncate())
                .on_hover_text(text);
        },
    );
}

pub fn tracks(
    ui: &mut egui::Ui,
    svc: Services<'_>,
    data: &mut Catalog,
    actions: &mut Actions,
    query: TrackQuery,
    lists: &[Playlist],
    active_source: Option<&str>,
) {
    let favorites = query == TrackQuery::Favorites;
    data.sync(ui, svc, &query);
    ui.horizontal_wrapped(|ui| {
        if let Some(error) = data.play_all(ui, svc) {
            actions.notice = Some((error, true));
        }
        if ui.button("刷新").clicked() {
            data.reset_tracks();
            actions.favorites.refresh();
        }
        ui.weak(format!(
            "{}{} 首",
            if data.tracks.next.is_some() {
                "已显示 "
            } else {
                "共 "
            },
            data.tracks.items.len()
        ));
        if data.tracks.busy() {
            ui.spinner();
        }
        actions.favorites.status(ui);
    });
    if let Some(error) = data.tracks.error.as_ref() {
        ui.colored_label(ui.visuals().error_fg_color, error);
        if ui.button("重试加载曲目").clicked() {
            data.more(ui, svc);
        }
    }
    if data.tracks.items.is_empty() {
        if !data.tracks.busy() && data.tracks.error.is_none() {
            let label = match query {
                TrackQuery::Library(ref q) if !q.is_empty() => "没有找到相关曲目",
                TrackQuery::Library(_) => "在服务端添加音乐目录并扫描后，曲目会出现在这里。",
                TrackQuery::Favorites => "收藏列表为空",
                TrackQuery::History => "暂无播放记录",
                TrackQuery::Playlist { .. } => "歌单中暂无曲目",
                TrackQuery::Category { .. } => "此分类暂无曲目",
            };
            ui.vertical_centered(|ui| {
                ui.add_space(72.0);
                ui.weak(label);
            });
        }
        return;
    }
    ui.add_space(6.0);
    let dark = ui.visuals().dark_mode;
    let muted = style::muted(dark);
    let width = ui.available_width() - 14.0;
    let show_album = width > 630.0;
    let artist_w = (width * 0.16).clamp(65.0, 180.0);
    let album_w = if show_album {
        (width * 0.16).clamp(70.0, 180.0)
    } else {
        0.0
    };
    let title_w = (width - artist_w - album_w - 224.0).max(55.0);
    egui::Frame::new()
        .fill(style::surface(dark))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (w, t) in [(34.0, "#"), (title_w, "曲名"), (artist_w, "歌手")] {
                    text_cell(ui, w, t, muted);
                }
                if show_album {
                    text_cell(ui, album_w, "专辑", muted);
                }
                text_cell(ui, 51.0, "时长", muted);
            });
        });
    let count = data.tracks.items.len();
    let mut play = None;
    let mut more = false;
    let extra = usize::from(data.tracks.next.is_some());
    egui::ScrollArea::vertical()
        .id_salt("track-data")
        .auto_shrink([false, false])
        .show_rows(ui, 37.0, count + extra, |ui, range| {
            for i in range {
                if i == count {
                    more = ui
                        .add_enabled(!data.tracks.busy(), egui::Button::new("加载更多曲目"))
                        .clicked();
                    continue;
                }
                let t = &data.tracks.items[i];
                let selected = active_source.is_some() && t.playable_source_id() == active_source;
                let text = if selected {
                    style::ACCENT
                } else {
                    ui.visuals().text_color()
                };
                let row = ui
                    .push_id((i, &t.id), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.add_space(3.0);
                            let playable = t.playable_source_id().is_some();
                            let index = if selected {
                                "▶".into()
                            } else {
                                format!("{:02}", i + 1)
                            };
                            if ui
                                .add_enabled(
                                    playable,
                                    egui::Button::new(RichText::new(index).color(if selected {
                                        style::ACCENT
                                    } else {
                                        muted
                                    }))
                                    .frame(false)
                                    .min_size(Vec2::new(34.0, 30.0)),
                                )
                                .on_hover_text(if playable {
                                    "播放曲目"
                                } else {
                                    "暂无可用音源"
                                })
                                .clicked()
                            {
                                play = Some(i);
                            }
                            text_cell(ui, title_w, &t.title, text);
                            let artist = t.artists.join(" / ");
                            text_cell(
                                ui,
                                artist_w,
                                if artist.is_empty() { "—" } else { &artist },
                                muted,
                            );
                            if show_album {
                                text_cell(ui, album_w, t.album.as_deref().unwrap_or("—"), muted);
                            }
                            text_cell(
                                ui,
                                51.0,
                                &time(t.source().and_then(|s| s.duration_ms)),
                                muted,
                            );
                            let favorite = actions.favorites.contains(&t.id, favorites);
                            let on = favorite == Some(true);
                            if ui
                                .add_enabled(
                                    favorite.is_some() && !actions.busy(),
                                    egui::Button::new(
                                        RichText::new(match favorite {
                                            Some(true) => "♥",
                                            Some(false) => "♡",
                                            None => "…",
                                        })
                                        .size(17.0)
                                        .color(if on {
                                            style::ACCENT
                                        } else {
                                            muted
                                        }),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text(match favorite {
                                    Some(true) => "取消收藏",
                                    Some(false) => "加入收藏",
                                    None => "尚未读取收藏状态",
                                })
                                .clicked()
                            {
                                actions.toggle(ui, svc, t, !on);
                            }
                            if ui
                                .add_enabled(playable, egui::Button::new("＋").frame(false))
                                .on_hover_text("加入歌单")
                                .clicked()
                            {
                                actions.open(t, lists);
                            }
                        })
                    })
                    .inner
                    .response;
                if ui
                    .interact(row.rect, row.id.with("double"), Sense::hover())
                    .hovered()
                    && ui.input(|i| {
                        i.pointer
                            .button_double_clicked(egui::PointerButton::Primary)
                    })
                    && t.playable_source_id().is_some()
                {
                    play = Some(i);
                }
            }
        });
    if more {
        data.more(ui, svc);
    }
    if let Some(i) = play {
        if let Err(e) = play_row(svc.player, &data.tracks.items, i) {
            actions.notice = Some((e, true));
        }
    }
}
