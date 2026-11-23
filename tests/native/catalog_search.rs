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
    catalog.open_credit(CategoryTab::Artist, "a".into(), "歌手".into());
    assert!(catalog.tabs == CategoryTab::Artist);
    assert_eq!(catalog.category, Some(("a".into(), "歌手".into())));
    assert_eq!(catalog.providers.selected.as_deref(), Some("local"));
    assert!(catalog.list_query.is_empty() && catalog.matching.selected_tracks().is_empty());
    assert!(!catalog.tracks.started);
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
        let mut actions = Actions::default();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 650.));
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(bounds),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let q = catalog.library(ui, svc).unwrap();
                    catalog.table_toolbar(ui, svc, &mut actions, &q, &[]);
                });
            },
        );
        let tree = output.platform_output.accesskit_update.unwrap();
        for label in ["新建曲目", "重复识别", "刷新列表", "默认顺序"] {
            let node = tree
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some(tr(label)))
                .unwrap_or_else(|| panic!("Missing {label}"))
                .1
                .bounds()
                .unwrap();
            assert!(
                node.x0 >= 0. && node.x1 <= width as f64,
                "{label} at {node:?} exceeds {width}"
            );
            if width >= 900. && matches!(label, "默认顺序" | "刷新列表") {
                assert!(
                    node.x1 > width as f64 - 60.,
                    "{label} at {node:?} ends before edge {width}"
                );
            }
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
