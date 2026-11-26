use crate::ui::{navigation::Screen, Services, SignedUi};
use eframe::egui::{
    self,
    accesskit::{Action, ActionRequest, NodeId, Role},
};
use musicfs_player_core::Player;
use musicfs_server_adapter::ServerSession;
use std::time::{Duration, Instant};

struct View<'a> {
    screen: SignedUi,
    ctx: egui::Context,
    svc: Services<'a>,
    name_focused: bool,
}

impl View<'_> {
    fn frame(&mut self, events: Vec<egui::Event>, button: &str) -> Option<NodeId> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            events,
            ..Default::default()
        };
        let update = self
            .ctx
            .run(input, |ctx| {
                self.screen
                    .show(ctx, self.svc, &Default::default(), false, None);
            })
            .platform_output
            .accesskit_update?;
        self.name_focused = update
            .nodes
            .iter()
            .any(|(id, node)| *id == update.focus && node.role() == Role::TextInput);
        update.nodes.into_iter().find_map(|(id, node)| {
            (node.role() == Role::Button && node.label() == Some(button) && !node.is_disabled())
                .then_some(id)
        })
    }

    fn click(&mut self, label: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(target) = self.frame(vec![], label) {
                self.frame(
                    vec![egui::Event::AccessKitActionRequest(ActionRequest {
                        action: Action::Click,
                        target,
                        data: None,
                    })],
                    "",
                );
                return;
            }
            assert!(Instant::now() < deadline, "Button not available: {label}");
            std::thread::sleep(Duration::from_millis(12));
        }
    }

    fn type_in_editor(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            self.frame(vec![], "");
            if self.name_focused {
                self.frame(vec![egui::Event::Text(text.to_owned())], "");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Playlist name field did not receive focus"
            );
            std::thread::sleep(Duration::from_millis(12));
        }
    }
}

// 从歌单管理创建空歌单，点击卡片打开详情，再验证编辑后的名称回写到共用侧栏列表。
#[test]
#[ignore = "requires a disposable local MusicFS server"]
fn creates_opens_and_renames_a_personal_playlist() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt
        .block_on(ServerSession::login(
            &endpoint,
            &std::env::var("MUSICFS_TEST_TOKEN").unwrap(),
        ))
        .unwrap();
    let activity = server.listen_activity(|| {});
    let player = Player::new(|_| Err("No playback requested".into())).unwrap();
    let mut view = View {
        screen: SignedUi::default(),
        ctx: egui::Context::default(),
        svc: Services {
            runtime: rt.handle(),
            activity: &activity,
            server: &server,
            player: &player,
        },
        name_focused: false,
    };
    view.ctx.enable_accesskit();
    view.screen.lists_closed = true;
    view.screen.navigate(Screen::Playlists);
    let name = format!("UI {}", uuid::Uuid::new_v4());
    view.click("新建歌单");
    view.type_in_editor(&name);
    view.click("创建歌单");
    view.click(&name);
    let Screen::Playlist(id) = view.screen.screen.clone() else {
        panic!("Playlist card did not open its detail page");
    };
    view.click("编辑歌单");
    view.type_in_editor(" renamed");
    view.click("保存修改");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        view.frame(vec![], "");
        if let Some(p) = view
            .screen
            .lists
            .items
            .iter()
            .find(|p| p.id == id && p.name.contains("renamed"))
        {
            let (published, _) = rt.block_on(server.playlist_rules(&id)).unwrap();
            assert_eq!(p.name, published.name);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Renamed playlist not reflected in the shared list"
        );
        std::thread::sleep(Duration::from_millis(12));
    }
    rt.block_on(server.logout()).unwrap();
}
