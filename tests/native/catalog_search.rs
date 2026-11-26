use super::*;
use egui::{
    accesskit::{Action, ActionRequest, Role, TreeUpdate},
    Context, Event, Key, Modifiers,
};

fn frame(
    ctx: &Context,
    catalog: &mut Catalog,
    modifiers: Modifiers,
    events: Vec<Event>,
) -> TreeUpdate {
    let input = egui::RawInput {
        modifiers,
        events,
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| catalog.list_search(ui));
    })
    .platform_output
    .accesskit_update
    .unwrap()
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn scroll_toolbar(pointer: egui::Pos2, delta: egui::Vec2) -> Vec<Event> {
    vec![
        Event::PointerMoved(pointer),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta,
            modifiers: Modifiers::NONE,
        },
    ]
}

#[test]
fn batch_selection_keeps_explicit_pins_and_credit_navigation_resets_the_scope() {
    let mut catalog = Catalog::default();
    let auto = Track {
        id: "song".into(),
        ..Default::default()
    };
    let pin = Track {
        forced_source_track_id: Some("live".into()),
        ..auto.clone()
    };
    catalog
        .matching
        .select_rows(&[auto.clone(), auto.clone(), pin.clone()], true);
    assert_eq!(catalog.matching.selected_tracks().len(), 2);
    catalog
        .matching
        .select_rows(std::slice::from_ref(&pin), false);
    assert!(catalog.matching.selected(&auto));
    assert!(!catalog.matching.selected(&pin));
    catalog.providers.selected = Some("local".into());
    catalog.list_query = "old".into();
    catalog.tracks.started = true;
    catalog.query = Some(TrackQuery::Category {
        tab: CategoryTab::Album,
        id: "album".into(),
        provider: catalog.providers.selected.clone(),
        query: catalog.list_query.clone(),
    });
    catalog.open_credit(CategoryTab::Artist, "a".into(), "歌手".into());
    assert!(catalog.tabs == CategoryTab::Artist);
    assert_eq!(catalog.category, Some(("a".into(), "歌手".into())));
    assert_eq!(catalog.providers.selected.as_deref(), Some("local"));
    assert!(catalog.list_query.is_empty() && catalog.matching.selected_tracks().is_empty());
    assert!(!catalog.tracks.started);

    catalog.query = Some(TrackQuery::AvailableSources {
        query: String::new(),
    });
    catalog.open_credit(CategoryTab::Album, "b".into(), "专辑".into());
    assert!(catalog.tabs == CategoryTab::Album);
    assert_eq!(catalog.category, Some(("b".into(), "专辑".into())));
    assert!(catalog.providers.selected.is_none());
}

// 独立实例包含扫描曲目，检查真实工具栏布局以及批量加入歌单的来源绑定。
#[test]
#[ignore = "requires an isolated MusicFS server"]
fn library_toolbar_fits_the_viewport_and_saves_bulk_references() {
    use super::super::actions::Actions;
    use musicfs_player_core::Player;
    use musicfs_server_adapter::ServerSession;
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt
        .block_on(ServerSession::login(
            &endpoint,
            &std::env::var("MUSICFS_TEST_TOKEN").unwrap(),
        ))
        .unwrap();
    let player = Player::new(|_| Err("No playback requested".into())).unwrap();
    let activity = server.listen_activity(|| {});
    let svc = Services {
        activity: &activity,
        runtime: rt.handle(),
        server: &server,
        player: &player,
    };
    for width in [460., 910., 1500.] {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        super::super::style::apply(&ctx, "dark", "default");
        ctx.enable_accesskit();
        let mut catalog = Catalog::default();
        catalog.providers.selected = Some("category-provider".into());
        let mut actions = Actions::default();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 650.));
        let mut now = 0.;
        let mut draw = |events, modifiers| {
            now += 1. / 30.;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(bounds),
                    events,
                    modifiers,
                    time: Some(now),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let q = catalog.library(ui, svc).unwrap();
                        assert!(
                            matches!(&q, TrackQuery::AvailableSources { query } if query.is_empty())
                        );
                        assert_eq!(
                            catalog.providers.selected.as_deref(),
                            Some("category-provider")
                        );
                        catalog.table_toolbar(ui, svc, &mut actions, &q, &[]);
                    });
                },
            )
            .platform_output
            .accesskit_update
            .unwrap()
        };
        draw(vec![], Modifiers::NONE);
        let mut tree = draw(vec![], Modifiers::NONE);
        let initial = tree
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(tr("播放全部")))
            .unwrap()
            .1
            .bounds()
            .unwrap();
        let line_y = (initial.y0 + initial.y1) / 2.;
        let pointer = egui::pos2(width / 2., line_y as f32);
        let expected = [
            "播放全部",
            "加入队列",
            "仅可播放",
            "更多筛选",
            "新建曲目",
            "一键合并重复音乐",
            "重复识别",
            "搜索当前列表",
            "刷新列表",
            "默认顺序",
        ];
        let mut seen = std::collections::HashSet::new();
        for _ in 0..45 {
            for (_, node) in &tree.nodes {
                let text = if node.role() == Role::TextInput {
                    tr("搜索当前列表")
                } else {
                    node.label().unwrap_or("")
                };
                assert!(!["来源", "全部来源", "全部格式", "仅收藏"]
                    .iter()
                    .any(|s| tr(s) == text));
                let Some(label) = expected.iter().find(|s| tr(s) == text) else {
                    continue;
                };
                let Some(rect) = node.bounds() else { continue };
                if rect.x0 >= 0. && rect.x1 <= f64::from(width) {
                    assert!(
                        ((rect.y0 + rect.y1) / 2. - line_y).abs() <= 3.,
                        "{label} at {rect:?} outside toolbar row {line_y}"
                    );
                    seen.insert(*label);
                    if *label == "默认顺序" && width == 1500. {
                        assert!(rect.x1 > f64::from(width) - 60.);
                    }
                }
            }
            if seen.len() == expected.len() || width == 1500. {
                break;
            }
            let delta = if width == 460. {
                egui::vec2(0., -45.)
            } else {
                egui::vec2(-45., 0.)
            };
            tree = draw(scroll_toolbar(pointer, delta), Modifiers::NONE);
        }
        assert_eq!(
            seen.len(),
            expected.len(),
            "Unreachable controls at width {width}, found {seen:?}"
        );
        if width == 460. {
            for _ in 0..40 {
                draw(
                    scroll_toolbar(pointer, egui::vec2(0., 60.)),
                    Modifiers::NONE,
                );
            }
            let shortcut = Modifiers::COMMAND | Modifiers::CTRL;
            draw(vec![key(Key::F, shortcut)], shortcut);
            for _ in 0..30 {
                tree = draw(vec![], Modifiers::NONE);
            }
            let focused = tree
                .nodes
                .iter()
                .find(|(id, n)| *id == tree.focus && n.role() == Role::TextInput)
                .unwrap();
            let field = focused.1.bounds().unwrap();
            assert!(
                field.x0 >= 0. && field.x1 <= f64::from(width),
                "Shortcut focused offscreen field: {field:?}"
            );
        }
    }
    rt.block_on(async {
        let t = server
            .sourced_tracks(None, None)
            .await
            .unwrap()
            .items
            .into_iter()
            .find(|t| t.playable_source_id().is_some())
            .unwrap();
        let pin = t.playable_source_id().unwrap().to_owned();
        let refs = vec![(t.id.clone(), None), (t.id.clone(), Some(pin.clone()))];
        let name = uuid::Uuid::new_v4().to_string();
        let key = uuid::Uuid::new_v4().to_string();
        for _ in 0..2 {
            server
                .add_track_references(None, &name, &refs, &key)
                .await
                .unwrap();
        }
        let lists = server.playlists(None).await.unwrap();
        let list = lists.items.iter().find(|p| p.name == name).unwrap();
        assert_eq!(lists.items.iter().filter(|p| p.name == name).count(), 1);
        let detail = server
            .settings_json("GET", &format!("/playlists/{}", list.id), None, &[])
            .await
            .unwrap();
        let entries = detail["bindings"][0]["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0]["source"]["forcedSourceTrackId"].is_null());
        assert_eq!(entries[1]["source"]["forcedSourceTrackId"], pin);
        server.logout().await.unwrap();
    });
}

#[test]
fn find_focuses_the_list_search_enter_submits_and_clear_restores_all_tracks() {
    for shortcut in [Modifiers::CTRL, Modifiers::MAC_CMD] {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut catalog = Catalog::default();
        catalog.categories.started = true;
        catalog.tracks.started = true;
        let initial = frame(&ctx, &mut catalog, Modifiers::NONE, vec![]);
        let field = initial
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::TextInput)
            .unwrap()
            .0;
        assert_ne!(initial.focus, field);

        let modifier = Modifiers::COMMAND | shortcut;
        let focused = frame(&ctx, &mut catalog, modifier, vec![key(Key::F, modifier)]);
        assert_eq!(focused.focus, field);
        frame(
            &ctx,
            &mut catalog,
            Modifiers::NONE,
            vec![Event::Text("  音乐 demo  ".into())],
        );
        assert_eq!(catalog.list_draft, "  音乐 demo  ");
        assert!(catalog.list_query.is_empty());
        assert!(catalog.categories.started && catalog.tracks.started);

        let submitted = frame(
            &ctx,
            &mut catalog,
            Modifiers::NONE,
            vec![key(Key::Enter, Modifiers::NONE)],
        );
        assert_eq!(catalog.list_query, "音乐 demo");
        assert!(!catalog.categories.started && !catalog.tracks.started);
        let clear = submitted
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some(tr("清除")))
            .unwrap()
            .0;
        catalog.categories.started = true;
        catalog.tracks.started = true;
        frame(
            &ctx,
            &mut catalog,
            Modifiers::NONE,
            vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target: clear,
                data: None,
            })],
        );
        assert!(catalog.list_draft.is_empty() && catalog.list_query.is_empty());
        assert!(!catalog.categories.started && !catalog.tracks.started);
    }
}
