use super::{layout::Columns, widgets::*, *};
use crate::icons;
use egui::{pos2, vec2, Rect, Sense, Vec2};
use musicfs_player_core::Status;

#[derive(Clone, Copy)]
pub(super) enum Action {
    Play,
    Next,
    Queue,
    Favorite,
    Add,
    Versions,
}
pub(super) enum RowEvent {
    Action(Action, bool),
    Credit(CategoryTab, String, String),
}

pub(super) fn show(
    ui: &mut egui::Ui,
    rect: Rect,
    c: &Columns,
    data: &mut Catalog,
    actions: &Actions,
    svc: Services<'_>,
    i: usize,
    favorites: bool,
    links: bool,
    active: Option<&str>,
    status: Status,
) -> Option<RowEvent> {
    let t = &data.tracks.items[i];
    let playable = t.playable_source_id().is_some();
    let current = active.is_some()
        && (t
            .source()
            .is_some_and(|s| Some(s.source_track_id.as_str()) == active)
            || (t.forced_source_track_id.is_none()
                && t.sources.as_ref().is_some_and(|g| {
                    g.choices()
                        .any(|s| Some(s.source_track_id.as_str()) == active)
                })));
    let dark = ui.visuals().dark_mode;
    let row = ui.interact(rect, ui.id().with("row"), Sense::click());
    let selected = data.matching.selected(t);
    if current || selected {
        ui.painter().rect_filled(rect, 0, style::accent_soft(ui));
    } else if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, 0, style::hovered(dark));
    }
    let [check, num, title, artist, album, provider, format, duration, buttons] = c.cells(rect);
    let mut check_ui = contents(ui, check_rect(check), "check");
    style::checkbox_style(&mut check_ui);
    data.matching.row(&mut check_ui, t);
    if current {
        playback_indicator(ui, num, status, i + 1);
    } else {
        label(
            ui,
            num,
            "number",
            &(i + 1).to_string(),
            style::muted(dark),
            12.,
        );
    }
    let mut result = None;
    let left = title.left() + if c.title > 150. { 14. } else { 4. };
    let cover = Rect::from_center_size(pos2(left + 21., rect.center().y), Vec2::splat(42.));
    data.artwork.paint(ui, cover, svc, t.cover());
    let tx = cover.right() + if c.title > 170. { 12. } else { 5. };
    let line = |y: f32| Rect::from_min_max(pos2(tx, y - 11.), pos2(title.right() - 3., y + 11.));
    let count = t.sources.as_ref().map_or(0, |g| g.choices().count());
    let artist_names = t.artists.join(" / ");
    let title_y = if count > 1 || !playable || c.artist == 0. {
        rect.center().y - 9.
    } else {
        rect.center().y
    };
    let name = if t.title.is_empty() {
        t.source().map_or("—", |s| s.display_name.as_str())
    } else {
        t.title.as_str()
    };
    if title_control(
        ui,
        line(title_y),
        "play",
        name,
        &tr("播放 {0}").replace("{0}", name),
        playable,
        false,
    )
    .clicked()
    {
        result = Some(RowEvent::Action(Action::Play, false));
    }
    let second = line(rect.center().y + 12.);
    if count > 1 {
        let description = tr("{0} 个来源版本").replace("{0}", &count.to_string());
        let open = data.versions.is_open(i);
        let toggle = tr(if open {
            "收起 {0} 的来源版本"
        } else {
            "展开 {0} 的来源版本"
        })
        .replace("{0}", name);
        if title_control(
            ui,
            second,
            if open { "chevron-up" } else { "chevron" },
            &description,
            &toggle,
            true,
            true,
        )
        .clicked()
        {
            result = Some(RowEvent::Action(Action::Versions, false));
        }
    } else if !playable {
        label(
            ui,
            second,
            "missing",
            tr("暂无可用音源"),
            style::muted(dark),
            11.,
        );
    } else if c.artist == 0. {
        label(
            ui,
            second,
            "artist-text",
            &artist_names,
            style::muted(dark),
            11.,
        );
    }
    if let Some(nav) = credit(
        ui,
        artist,
        &t.artist_credits,
        if artist_names.is_empty() {
            "—"
        } else {
            &artist_names
        },
        links,
        CategoryTab::Artist,
    ) {
        result = Some(nav);
    }
    if let Some(nav) = credit(
        ui,
        album,
        &t.album_credits,
        t.album.as_deref().unwrap_or("—"),
        links,
        CategoryTab::Album,
    ) {
        result = Some(nav);
    }
    let source = t.source();
    let provider_name = source.map_or("—", |s| data.providers.label(&s.provider_instance_id));
    label(
        ui,
        provider,
        "provider",
        provider_name,
        ui.visuals().text_color(),
        13.,
    );
    let fmt = source
        .and_then(|s| s.container.as_ref().or(s.codec.as_ref()))
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| "—".into());
    label(ui, format, "format", &fmt, ui.visuals().text_color(), 11.);
    label(
        ui,
        duration,
        "duration",
        &time(source.and_then(|s| s.duration_ms)),
        style::muted(dark),
        13.,
    );
    let fav = actions.favorites.contains(&t.id, favorites);
    let saved = fav == Some(true);
    let favorite_label = tr(if saved {
        "取消收藏"
    } else {
        "加入收藏"
    });
    let favorite_enabled = fav.is_some() && !t.unresolved && !actions.busy();
    let mut bar = contents(ui, buttons.shrink2(vec2(4., 0.)), "actions");
    bar.spacing_mut().item_spacing.x = 1.;
    let control = |ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool| {
        ui.add_enabled_ui(enabled, |ui| icons::button(ui, icon, label, 38., 18.))
            .inner
    };
    if buttons.width() >= 85. {
        let r = bar
            .scope(|ui| {
                if saved {
                    let accent = style::accent(ui);
                    let w = &mut ui.visuals_mut().widgets;
                    w.inactive.fg_stroke.color = accent;
                    w.hovered.fg_stroke.color = accent;
                    w.active.fg_stroke.color = accent;
                }
                control(
                    ui,
                    if saved { "heart-filled" } else { "heart" },
                    favorite_label,
                    favorite_enabled,
                )
            })
            .inner;
        if r.clicked() {
            result = Some(RowEvent::Action(Action::Favorite, false));
        }
    }
    if buttons.width() >= 158. {
        if control(&mut bar, "next", tr("下一首播放"), playable).clicked() {
            result = Some(RowEvent::Action(Action::Next, false));
        }
        if control(&mut bar, "plus", tr("加入歌单"), !t.unresolved).clicked() {
            result = Some(RowEvent::Action(Action::Add, false));
        }
    }
    let more = control(&mut bar, "more", tr("更多操作"), true);
    let popup = more.id.with("commands");
    let right_click = ui.rect_contains_pointer(rect) && ui.input(|i| i.pointer.secondary_clicked());
    let keyboard =
        row.has_focus() && ui.input(|i| i.modifiers.shift && i.key_pressed(egui::Key::F10));
    if more.clicked() || right_click || keyboard {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    egui::popup_below_widget(
        ui,
        popup,
        &more,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(145.);
            let multi = selected && data.matching.selected_tracks().len() > 1;
            let group_can_play = playable
                || (multi
                    && data
                        .matching
                        .selected_tracks()
                        .iter()
                        .any(|r| r.playable_source_id().is_some()));
            for (action, label, enabled) in [
                (Action::Play, tr("播放"), group_can_play),
                (Action::Next, tr("下一首播放"), group_can_play),
                (Action::Queue, tr("加入队列"), group_can_play),
                (
                    Action::Favorite,
                    if multi {
                        tr("加入收藏")
                    } else {
                        favorite_label
                    },
                    if multi {
                        actions.can_favorite_many(data.matching.selected_tracks())
                    } else {
                        favorite_enabled
                    },
                ),
                (Action::Add, tr("加入歌单"), !t.unresolved),
                (Action::Versions, tr("来源版本"), count > 0 && !multi),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(label).frame(false))
                    .clicked()
                {
                    result = Some(RowEvent::Action(action, multi));
                }
            }
        },
    );
    if row.double_clicked() && playable {
        result = Some(RowEvent::Action(Action::Play, false));
    }
    result
}
