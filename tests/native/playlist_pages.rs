use super::*;
use eframe::egui::accesskit::{Action, ActionRequest, NodeId};
use musicfs_player_core::Player;
use musicfs_server_adapter::ServerSession;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn frame(
    screen: &mut SignedUi,
    ctx: &egui::Context,
    svc: Services<'_>,
    click: Option<NodeId>,
) -> Option<NodeId> {
    let mut input = egui::RawInput::default();
    if let Some(target) = click {
        input
            .events
            .push(egui::Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target,
                data: None,
            }));
    }
    let output = ctx.run(input, |ctx| {
        screen.show(ctx, svc, "dark", false, None);
    });
    output
        .platform_output
        .accesskit_update?
        .nodes
        .into_iter()
        .find(|(_, node)| node.label() == Some("读取更多目标歌单"))
        .map(|(id, _)| id)
}

fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "Desktop page did not finish loading"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

// 此测试会创建 51 个歌单并发布其中一个，仅允许使用有扫描曲目的隔离实例。
#[test]
#[ignore]
fn opens_playlist_from_second_page_after_refreshing_the_directory() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (server, track, target) = rt.block_on(async {
        let server = ServerSession::login(&endpoint, &std::env::var("MUSICFS_TEST_TOKEN").unwrap())
            .await
            .unwrap();
        let track = server
            .tracks("", None)
            .await
            .unwrap()
            .items
            .into_iter()
            .find(|t| t.source_track_id.is_some())
            .unwrap();
        let source = [track.source_track_id.clone().unwrap()];
        for i in 0..51 {
            server
                .create_playlist(
                    &format!("Pagination {i:02}"),
                    &source,
                    &Uuid::new_v4().to_string(),
                )
                .await
                .unwrap();
        }
        let after = server.playlists(None).await.unwrap().next_cursor.unwrap();
        let target = server
            .playlists(Some(&after))
            .await
            .unwrap()
            .items
            .into_iter()
            .next()
            .unwrap();
        server
            .rebuild_playlist(
                &target.id,
                target.rule_revision,
                &Uuid::new_v4().to_string(),
            )
            .await
            .unwrap();
        (server, track, target)
    });
    let player = Player::new(|_| Err("No playback requested".into())).unwrap();
    let svc = Services {
        server: &server,
        runtime: rt.handle(),
        player: &player,
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut screen = SignedUi::default();
    wait(|| {
        frame(&mut screen, &ctx, svc, None);
        screen.lists.next.is_some()
    });
    assert!(!screen.lists.items.iter().any(|p| p.id == target.id));
    screen.actions.open(&track, &screen.lists.items);
    let mut button = None;
    wait(|| {
        button = frame(&mut screen, &ctx, svc, None);
        button.is_some()
    });
    frame(&mut screen, &ctx, svc, button);
    wait(|| {
        frame(&mut screen, &ctx, svc, None);
        screen.lists.items.iter().any(|p| p.id == target.id)
    });
    screen.actions = Actions::default();
    screen.navigate(Screen::Playlist(target.id));
    screen.lists = Pages::default();
    wait(|| {
        frame(&mut screen, &ctx, svc, None);
        screen.catalog.tracks.items.iter().any(|t| t.id == track.id)
    });
    rt.block_on(server.logout()).unwrap();
}
