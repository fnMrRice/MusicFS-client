use super::*;
use egui::{
    accesskit::{Action, ActionRequest, Role, TreeUpdate},
    Context, Event, Key, Modifiers,
};

#[test]
fn category_tabs_leave_space_on_both_sides_of_the_text() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut catalog = Catalog::default();
    let mut bounds = Vec::new();
    let output = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            catalog.category_tabs(ui);
            for tab in [CategoryTab::Artist, CategoryTab::Album, CategoryTab::Genre] {
                let text = tab.label();
                let width = ui
                    .painter()
                    .layout_no_wrap(
                        text.into(),
                        egui::FontId::proportional(16.),
                        egui::Color32::WHITE,
                    )
                    .size()
                    .x;
                bounds.push((text, width));
            }
        });
    });
    let tree = output.platform_output.accesskit_update.unwrap();
    for (text, width) in bounds {
        let rect = tree
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.label() == Some(text))
                    .then(|| node.bounds())
                    .flatten()
            })
            .unwrap();
        assert!(
            rect.width() as f32 >= width + 34.,
            "Tab missing horizontal padding: {text}"
        );
    }
}

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
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| catalog.list_search(ui, tr("刷新列表")));
        });
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

#[test]
fn category_search_and_refresh_stay_together_at_the_right_edge() {
    for width in [330., 960.] {
        for draft in ["", "demo"] {
            let ctx = Context::default();
            ctx.enable_accesskit();
            let mut catalog = Catalog::default();
            catalog.list_draft = draft.into();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 180.),
                )),
                ..Default::default()
            };
            let mut right = 0.;
            let mut left_actions = egui::Rect::NOTHING;
            let output = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    right = f64::from(ui.max_rect().right());
                    ui.horizontal_wrapped(|ui| {
                        left_actions = ui.allocate_space(egui::vec2(250., 36.)).1;
                        catalog.list_search(ui, tr("刷新分类"));
                    });
                });
            });
            let tree = output.platform_output.accesskit_update.unwrap();
            let button = |label| {
                tree.nodes
                    .iter()
                    .find(|(_, n)| n.role() == Role::Button && n.label() == Some(tr(label)))
                    .and_then(|(_, n)| n.bounds())
            };
            let search = tree
                .nodes
                .iter()
                .find(|(_, n)| n.role() == Role::TextInput)
                .unwrap()
                .1
                .bounds()
                .unwrap();
            let refresh = button("刷新分类").unwrap();
            assert!(
                (refresh.x1 - right).abs() <= 1.,
                "Refresh at {refresh:?}, right edge {right}"
            );
            assert!(search.x1 <= refresh.x0 && search.y0 < refresh.y1 && refresh.y0 < search.y1);
            if width == 330. {
                assert!(search.y0 >= f64::from(left_actions.bottom()));
            } else {
                assert!(search.x0 >= f64::from(left_actions.right()));
            }
            let clear = button("清除");
            assert_eq!(clear.is_some(), !draft.is_empty());
            if let Some(clear) = clear {
                assert!(search.x1 <= clear.x0 && clear.x1 <= refresh.x0);
                assert!(clear.y0 < refresh.y1 && refresh.y0 < clear.y1);
            }
        }
    }
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
    catalog.list_query = "old".into();
    catalog.tracks.started = true;
    catalog.query = Some(TrackQuery::Category {
        tab: CategoryTab::Album,
        id: "album".into(),
        query: catalog.list_query.clone(),
    });
    catalog.open_credit(CategoryTab::Artist, "a".into(), "歌手".into());
    assert!(catalog.tabs == CategoryTab::Artist);
    assert_eq!(catalog.category, Some(("a".into(), "歌手".into())));
    assert!(catalog.list_query.is_empty() && catalog.matching.selected_tracks().is_empty());
    assert!(!catalog.tracks.started);

    catalog.query = Some(TrackQuery::AvailableSources {
        query: String::new(),
    });
    catalog.open_credit(CategoryTab::Album, "b".into(), "专辑".into());
    assert!(catalog.tabs == CategoryTab::Album);
    assert_eq!(catalog.category, Some(("b".into(), "专辑".into())));
    assert!(catalog.query.is_none());
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
    let rows = rt
        .block_on(server.sourced_tracks(None, None))
        .unwrap()
        .items;
    let pair: Vec<Track> = rows
        .iter()
        .filter(|t| t.source().is_some())
        .take(2)
        .cloned()
        .collect();
    assert_eq!(pair.len(), 2);
    // 使用主窗口的默认尺寸、侧栏宽度和内容边距检查整行。
    for width in [1232., 1440., 1920.] {
        let ctx = Context::default();
        crate::fonts::install(&ctx);
        super::super::style::apply(&ctx, "dark", "default");
        ctx.enable_accesskit();
        let mut catalog = Catalog::default();
        let mut actions = Actions::default();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.));
        let shortcut = Modifiers::COMMAND | Modifiers::CTRL;
        let mut draw = |catalog: &mut Catalog, events, modifiers| {
            let input = egui::RawInput {
                screen_rect: Some(bounds),
                modifiers,
                events,
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| {
                egui::SidePanel::left("sidebar")
                    .exact_width(240.)
                    .show(ctx, |_| {});
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(30, 0)))
                    .show(ctx, |ui| {
                        let q = catalog.library(ui, svc).unwrap();
                        assert!(
                            matches!(&q, TrackQuery::AvailableSources { query } if query.is_empty())
                        );
                        catalog.table_toolbar(ui, svc, &mut actions, &q, &[]);
                    });
            });
            output.platform_output.accesskit_update.unwrap()
        };
        let tree = draw(&mut catalog, vec![key(Key::F, shortcut)], shortcut);
        let (search_id, search) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::TextInput)
            .unwrap();
        assert_eq!(*search_id, tree.focus);
        let search = search.bounds().unwrap();
        let line_y = (search.y0 + search.y1) / 2.;
        let visible = |r: egui::accesskit::Rect| r.x0 >= 270. && r.x1 <= f64::from(width) - 29.;
        assert!(visible(search), "Search at {search:?} outside {width}");
        for label in [
            "播放全部",
            "加入队列",
            "仅可播放",
            "更多筛选",
            "新建曲目",
            "一键合并重复音乐",
            "重复识别",
            "刷新列表",
            "默认顺序",
        ] {
            let node = tree
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some(tr(label)))
                .unwrap_or_else(|| panic!("Missing {label}"))
                .1
                .bounds()
                .unwrap();
            assert!(visible(node), "{label} at {node:?} outside {width}");
            assert!(
                ((node.y0 + node.y1) / 2. - line_y).abs() <= 3.,
                "{label} is outside toolbar row"
            );
        }
        for label in ["来源", "全部来源", "全部格式", "仅收藏"] {
            assert!(!tree.nodes.iter().any(|(_, n)| n.label() == Some(tr(label))));
        }
        if width == 1232. {
            catalog.matching.select_rows(&pair, true);
            let tree = draw(&mut catalog, vec![], Modifiers::NONE);
            let menu = tree
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some(tr("曲目关联")))
                .unwrap()
                .0;
            let tree = draw(
                &mut catalog,
                vec![Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target: menu,
                    data: None,
                })],
                Modifiers::NONE,
            );
            for label in ["合并歌曲", "不是同一录音", "撤销排除"] {
                assert!(
                    tree.nodes.iter().any(|(_, n)| n.label() == Some(tr(label))),
                    "Missing {label}"
                );
            }
        }
    }
    rt.block_on(async {
        let t = rows
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
