use super::*;
use egui::accesskit::{Action, ActionRequest, NodeId, TreeUpdate};
use musicfs_player_core::Player;
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

fn frame(
    screen: &mut SignedUi,
    ctx: &egui::Context,
    svc: Services<'_>,
    click: Option<NodeId>,
) -> TreeUpdate {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280., 1200.),
        )),
        events: click
            .into_iter()
            .map(|target| {
                egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target,
                    data: None,
                })
            })
            .collect(),
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        screen.playlists.sync_sources(ctx, svc);
        screen.sidebar(ctx, svc);
        if matches!(screen.screen, Screen::SourcePlaylist(_) | Screen::Playlists) {
            egui::CentralPanel::default().show(ctx, |ui| screen.page(ui, svc));
        }
    })
    .platform_output
    .accesskit_update
    .unwrap()
}

fn wait_for(label: &str, mut frame: impl FnMut() -> TreeUpdate) -> NodeId {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let tree = frame();
        if let Some((id, _)) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label) || n.value() == Some(label))
        {
            return *id;
        }
        assert!(Instant::now() < deadline, "Missing UI item {label:?}");
        std::thread::sleep(Duration::from_millis(8));
    }
}

#[test]
fn sidebar_lists_and_opens_account_playlists_without_importing() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (record, calls) = mpsc::channel();
    let swapped = Arc::new(AtomicBool::new(false));
    let switched = Arc::clone(&swapped);
    let server = rt.block_on(http_fixture::session(move |url| match url.path() {
        "/api/v1/online-instances" => json!({"items":[{
            "instanceId":"netease","platform":"netease","name":"NetEase","authentication":"platform",
            "availability":"available","accountId": if switched.load(Ordering::SeqCst) {"other"} else {"sample"},
            "capabilities":[{"name":"playlist.list","supported":true},{"name":"playlist.observe","supported":true},
                            {"name":"stream.resolve","supported":true}]
        },{"instanceId":"offline","availability":"offline","capabilities":[{"name":"playlist.list","supported":true}]}]}),
        "/api/v1/online-instances/netease/playlists" => {
            let more = url.query_pairs().any(|(k,v)| k=="cursor" && v=="second");
            record.send(if more {"second"} else {"first"}).unwrap();
            if switched.load(Ordering::SeqCst) {
                json!({"items":[{"externalId":"saved","title":"Other account list","writable":true}]})
            } else if more {
                json!({"items":[{"externalId":"saved","title":"Saved collection","writable":false}]})
            } else {
                json!({"items":[{"externalId":"owned","title":"Owned mix","writable":true}],"nextCursor":"second"})
            }
        },
        "/api/v1/online-instances/netease/playlists/saved/tracks" => {
            record.send("detail").unwrap();
            json!({"playlist":{"externalId":"saved","title":"Saved collection"},"complete":true,"observedAt":"2026-11-26",
                "items":[{"position":0,"externalId":"sample-track","track":{"externalId":"sample-track","title":"Sample song","artists":["Sample artist"]}}]})
        },
        "/api/v1/online-instances/netease/tracks/sample-track/resolve" => {
            record.send("resolve").unwrap();
            json!({"sourceTrackId":"resolved","providerInstanceId":"netease","assetId":"asset","availability":"available"})
        }
        "/api/v1/online-instances/offline/playlists" => panic!("Requested offline account"),
        _ => json!({"items":[]}),
    }));
    let player = Player::new(|_| Err("No media in fixture".into())).unwrap();
    let activity = server.listen_activity(|| {});
    let svc = Services {
        server: &server,
        activity: &activity,
        player: &player,
        runtime: rt.handle(),
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.style_mut(|s| s.animation_time = 0.);
    let mut screen = SignedUi::default();
    wait_for("Owned mix", || frame(&mut screen, &ctx, svc, None));
    let saved = wait_for(crate::i18n::tr("收藏的歌单"), || {
        frame(&mut screen, &ctx, svc, None)
    });
    frame(&mut screen, &ctx, svc, Some(saved));
    let item = wait_for("Saved collection", || frame(&mut screen, &ctx, svc, None));
    frame(&mut screen, &ctx, svc, Some(item));
    assert!(matches!(&screen.screen, Screen::SourcePlaylist(key) if key.external == "saved"));
    assert_eq!(screen.title(), "Saved collection");
    wait_for("Sample song", || frame(&mut screen, &ctx, svc, None));
    let play = wait_for(crate::i18n::tr("播放"), || {
        frame(&mut screen, &ctx, svc, None)
    });
    frame(&mut screen, &ctx, svc, Some(play));
    let deadline = Instant::now() + Duration::from_secs(8);
    while player.snapshot().unwrap().queue.is_empty() {
        frame(&mut screen, &ctx, svc, None);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(8));
    }
    assert_eq!(
        player.snapshot().unwrap().queue[0]
            .source_track_id
            .as_deref(),
        Some("resolved")
    );
    assert_eq!(
        calls.try_iter().collect::<Vec<_>>(),
        ["first", "second", "detail", "resolve"]
    );
    // 即使两个账号拥有外部编号相同的歌单，历史路由也不能打开切换后的账号数据。
    swapped.store(true, Ordering::SeqCst);
    screen.playlists.invalidate_sources();
    wait_for("Other account list", || frame(&mut screen, &ctx, svc, None));
    wait_for(
        crate::i18n::tr("此歌单已删除或无法访问。"),
        || frame(&mut screen, &ctx, svc, None),
    );
    assert_eq!(calls.try_iter().collect::<Vec<_>>(), ["first"]);
}
