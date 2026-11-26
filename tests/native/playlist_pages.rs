use super::*;
use eframe::egui::accesskit::{Role, TreeUpdate};
use musicfs_player_core::Player;
use musicfs_server_adapter::ServerSession;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn frame(
    screen: &mut SignedUi,
    ctx: &egui::Context,
    svc: Services<'_>,
    events: Vec<egui::Event>,
) -> TreeUpdate {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800., 720.),
        )),
        events,
        ..Default::default()
    };
    let output = ctx.run(input, |ctx| {
        screen.show(ctx, svc, &Default::default(), false, None);
    });
    output.platform_output.accesskit_update.unwrap()
}

fn wait(stage: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "Timed out waiting for {stage}");
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
            .find(|t| t.playable_source_id().is_some())
            .unwrap();
        let source = [track.playable_source_id().map(str::to_owned).unwrap()];
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
    let activity = server.listen_activity(|| {});
    let svc = Services {
        activity: &activity,
        server: &server,
        runtime: rt.handle(),
        player: &player,
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut screen = SignedUi::default();
    screen.lists_closed = true;
    wait("the playlist directory", || {
        frame(&mut screen, &ctx, svc, vec![]);
        screen.lists.next.is_some()
    });
    assert!(!screen.lists.items.iter().any(|p| p.id == target.id));
    screen.actions.open(&track, &screen.lists.items);
    // 等待编辑窗口完成初次测量后，在目标歌单下拉框内滚到底部。
    frame(&mut screen, &ctx, svc, vec![]);
    let tree = frame(&mut screen, &ctx, svc, vec![]);
    let bounds = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == Role::ComboBox && node.label() == Some(crate::i18n::tr("目标歌单"))
        })
        .unwrap()
        .1
        .bounds()
        .unwrap();
    let pos = egui::pos2(
        (bounds.x0 + bounds.x1) as f32 / 2.,
        (bounds.y0 + bounds.y1) as f32 / 2.,
    );
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &mut screen,
        &ctx,
        svc,
        vec![egui::Event::PointerMoved(pos), press(true)],
    );
    frame(&mut screen, &ctx, svc, vec![press(false)]);
    frame(&mut screen, &ctx, svc, vec![]);
    let tree = frame(&mut screen, &ctx, svc, vec![]);
    let option = tree
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(crate::i18n::tr("新建歌单…")))
        .expect("Playlist picker did not open")
        .1
        .bounds()
        .unwrap();
    let pos = egui::pos2(
        (option.x0 + option.x1) as f32 / 2.,
        (option.y0 + option.y1) as f32 / 2.,
    );
    frame(
        &mut screen,
        &ctx,
        svc,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Page,
                delta: egui::vec2(0., -10.),
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    wait("the second page of the playlist picker", || {
        frame(&mut screen, &ctx, svc, vec![]);
        screen.lists.items.iter().any(|p| p.id == target.id)
    });
    screen.actions = Actions::default();
    screen.navigate(Screen::Playlist(target.id));
    screen.lists = Pages::default();
    wait("the selected playlist tracks", || {
        frame(&mut screen, &ctx, svc, vec![]);
        screen.catalog.tracks.items.iter().any(|t| t.id == track.id)
    });
    rt.block_on(server.logout()).unwrap();
}
