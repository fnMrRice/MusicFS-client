use super::{http_fixture, Actions, PlayerUi, Services};
use crate::i18n::tr;
use eframe::egui::{
    self,
    accesskit::{Action, ActionRequest, Node, NodeId, TreeUpdate},
    Event,
};
use musicfs_player_core::{Player, Track};
use serde_json::json;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn track(id: &str, title: &str, source_only: bool) -> Track {
    serde_json::from_value(json!({
        "id":id, "title":title, "sourceOnly":source_only, "artists":[],
        "sourceTrackId":id, "durationMs":60000
    }))
    .unwrap()
}

fn control<'a>(tree: &'a TreeUpdate, label: &str) -> (&'a NodeId, &'a Node) {
    tree.nodes
        .iter()
        .find(|(_, n)| n.label() == Some(label) || n.value() == Some(label))
        .map(|(id, n)| (id, n))
        .unwrap_or_else(|| panic!("Missing {label}"))
}

#[test]
fn player_bar_positions_and_favorites_follow_the_displayed_track() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (tx, writes) = mpsc::channel();
    let server = rt.block_on(http_fixture::session_with_response(move |method, url| {
        let body = match url.path() {
            "/api/v1/me/favorites" => json!({"items":[], "nextCursor":null}),
            "/api/v1/library/sources/cloud/track" => json!({
                "trackId":"canonical", "title":"Cloud", "artists":[], "albums":[]
            }),
            "/api/v1/me/favorites/canonical" => {
                tx.send(method.to_owned()).unwrap();
                json!({})
            }
            _ => json!({}),
        };
        ("200 OK", "application/json", body.to_string().into_bytes())
    }));
    let activity = server.listen_activity(|| {});
    let player = Player::new(|_| Err("No audio requested".into())).unwrap();
    let svc = Services {
        runtime: rt.handle(),
        server: &server,
        activity: &activity,
        player: &player,
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut panel = PlayerUi::default();
    let mut actions = Actions::default();
    let order = tr("播放顺序：{0}").replace("{0}", tr("顺序播放"));
    let long = "Very long track name ".repeat(12);
    player
        .set_queue(vec![track("cloud", "Cloud", true)], 0)
        .unwrap();
    let frame = |panel: &mut PlayerUi, actions: &mut Actions, events| {
        panel.refresh();
        actions.poll();
        actions.favorites.sync(&ctx, svc.runtime, svc.server);
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                panel.show(ctx, &player, &server, rt.handle(), 0.8, 0.8, |ui, track| {
                    actions.player_favorite(ui, svc, track)
                });
            },
        )
        .platform_output
        .accesskit_update
        .unwrap()
    };
    let ready = |panel: &mut PlayerUi, actions: &mut Actions, label: &str| {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let tree = frame(panel, actions, vec![]);
            if tree
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some(label) && !n.is_disabled())
            {
                return tree;
            }
            assert!(
                Instant::now() < deadline,
                "Missing enabled control: {label}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let tree = ready(&mut panel, &mut actions, tr("加入收藏"));
    let (heart_id, heart) = control(&tree, tr("加入收藏"));
    let heart = heart.bounds().unwrap();
    let title = control(&tree, "Cloud").1.bounds().unwrap();
    let mode = control(&tree, &order).1.bounds().unwrap();
    let prev = control(&tree, tr("上一首")).1.bounds().unwrap();
    assert!(heart.x0 >= title.x1 && heart.x0 - title.x1 < 8.);
    assert!(heart.y0 < title.y1 && title.y0 < heart.y1);
    assert!(mode.x1 <= prev.x0);
    assert!(!tree
        .nodes
        .iter()
        .any(|(_, n)| n.label() == Some(tr("迷你播放器"))));
    let click = |id| {
        vec![Event::AccessKitActionRequest(ActionRequest {
            action: Action::Click,
            target: id,
            data: None,
        })]
    };
    frame(&mut panel, &mut actions, click(*heart_id));
    player
        .set_queue(vec![track("other", &long, false)], 0)
        .unwrap();
    let tree = ready(&mut panel, &mut actions, tr("加入收藏"));
    let heart = control(&tree, tr("加入收藏")).1.bounds().unwrap();
    assert!(heart.x1 < control(&tree, &order).1.bounds().unwrap().x0);
    assert_eq!(writes.recv_timeout(Duration::from_secs(1)).unwrap(), "PUT");
    assert_eq!(actions.favorites.contains("canonical", false), Some(true));
    assert_eq!(actions.favorites.contains("other", false), Some(false));
    player
        .set_queue(vec![track("canonical", "Library", false)], 0)
        .unwrap();
    let tree = ready(&mut panel, &mut actions, tr("取消收藏"));
    frame(
        &mut panel,
        &mut actions,
        click(*control(&tree, tr("取消收藏")).0),
    );
    ready(&mut panel, &mut actions, tr("加入收藏"));
    assert_eq!(
        writes.recv_timeout(Duration::from_secs(1)).unwrap(),
        "DELETE"
    );
}
