use super::{
    widgets::{credit, title_control},
    *,
};
use eframe::egui::{
    accesskit::{Action, ActionRequest, Role, TreeUpdate},
    Context, Event, Rect,
};
use musicfs_server_adapter::TrackCredit;

#[test]
fn favorited_heart_uses_the_theme_color_without_a_permanent_tile() {
    for theme in ["dark", "light"] {
        for saved in [false, true] {
            let ctx = Context::default();
            style::apply(&ctx, theme, "miku");
            let mut target = Rect::NOTHING;
            let mut ink = egui::Color32::TRANSPARENT;
            let output = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    target = widgets::favorite_button(ui, saved, "Favorite").rect;
                    ink = if saved {
                        style::accent_fill(ui)
                    } else {
                        style::text(ui.visuals().dark_mode)
                    };
                });
            });
            assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Mesh(m) if !m.vertices.is_empty() && m.vertices.iter().all(|v| v.color == ink && target.contains(v.pos))
            )));
            assert!(!output.shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Rect(r) if r.rect == target && r.fill != egui::Color32::TRANSPARENT
            )));
        }
    }
}

#[derive(Default)]
struct Clicks {
    play: bool,
    artist: Option<String>,
    disclosure: bool,
}

fn frame(ctx: &Context, events: Vec<Event>, enabled: bool) -> (TreeUpdate, Clicks) {
    let mut clicks = Clicks::default();
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = |y: f32| Rect::from_min_size(egui::pos2(20., y), egui::vec2(210., 24.));
            clicks.play =
                title_control(ui, rect(25.), "play", "Track", "Play Track", enabled, false)
                    .clicked();
            clicks.disclosure = title_control(
                ui,
                rect(56.),
                "chevron",
                "2 sources",
                "Expand sources",
                true,
                true,
            )
            .clicked();
            let credit_event = credit(
                ui,
                rect(90.),
                &[TrackCredit {
                    id: "artist-42".into(),
                    name: "Artist".into(),
                }],
                "Artist",
                true,
                CategoryTab::Artist,
            );
            if let Some(RowEvent::Credit(CategoryTab::Artist, id, _)) = credit_event {
                clicks.artist = Some(id);
            }
        });
    });
    (output.platform_output.accesskit_update.unwrap(), clicks)
}

#[test]
fn keyboard_accessible_row_controls_dispatch_only_their_own_action() {
    for (label, role) in [
        ("Play Track", Role::Button),
        ("Expand sources", Role::Button),
        ("Artist", Role::Link),
    ] {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let (tree, _) = frame(&ctx, vec![], true);
        let (id, _) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == role && n.label() == Some(label))
            .unwrap();
        let (_, clicked) = frame(
            &ctx,
            vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target: *id,
                data: None,
            })],
            true,
        );
        assert_eq!(clicked.play, label == "Play Track");
        assert_eq!(clicked.disclosure, label == "Expand sources");
        assert_eq!(
            clicked.artist.as_deref(),
            (label == "Artist").then_some("artist-42")
        );
    }
}

#[test]
#[ignore = "requires an isolated MusicFS server with a single-source track"]
fn single_source_row_expands_and_collapses() {
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
    let track = rt
        .block_on(server.sourced_tracks(None, None))
        .unwrap()
        .items
        .into_iter()
        .find(|t| t.sources.as_ref().is_some_and(|g| g.choices().count() == 1))
        .expect("Expected a single-source track");
    let player = Player::new(|_| Err("No playback requested".into())).unwrap();
    let activity = server.listen_activity(|| {});
    let svc = Services {
        runtime: rt.handle(),
        activity: &activity,
        server: &server,
        player: &player,
    };
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut catalog = Catalog::default();
    let mut actions = Actions::default();
    let name = track.title.clone();
    catalog.tracks.items.push(track);
    let mut frame = |catalog: &mut Catalog, events: Vec<Event>| {
        let tree = ctx
            .run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(960., HEIGHT), egui::Sense::hover());
                        let cols = Columns::new(rect.width(), false);
                        if let Some(event) = row::show(
                            ui,
                            rect,
                            &cols,
                            catalog,
                            &actions,
                            svc,
                            0,
                            false,
                            true,
                            None,
                            Status::Idle,
                        ) {
                            apply(ui, svc, catalog, &mut actions, &[], 0, event, false);
                        }
                    });
                },
            )
            .platform_output
            .accesskit_update
            .unwrap();
        (tree, catalog.versions.expanded(&catalog.tracks.items))
    };
    for (label, expanded) in [
        ("展开 {0} 的来源版本", true),
        ("收起 {0} 的来源版本", false),
    ] {
        let (tree, _) = frame(&mut catalog, vec![]);
        let label = tr(label).replace("{0}", &name);
        let (target, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::Button && node.label() == Some(&label))
            .expect("Expected the source disclosure button");
        let (_, rows) = frame(
            &mut catalog,
            vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target: *target,
                data: None,
            })],
        );
        assert_eq!(rows, if expanded { vec![(0, 1)] } else { vec![] });
    }
    catalog.tracks.items[0].sources = None;
    let (tree, _) = frame(&mut catalog, vec![]);
    assert!(tree
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some(tr("暂无可用音源"))
            || node.value() == Some(tr("暂无可用音源"))));
}
