use super::*;
use egui::{
    accesskit::{Action, ActionRequest, NodeId, Role, TreeUpdate},
    Event,
};
use musicfs_player_core::Player;
use serde_json::{json, Value};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

struct Library<'a> {
    ui: SignedUi,
    ctx: egui::Context,
    services: Services<'a>,
}

impl Library<'_> {
    fn frame(&mut self, events: Vec<Event>) -> TreeUpdate {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 720.),
            )),
            events,
            ..Default::default()
        };
        self.ctx
            .run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| self.ui.page(ui, self.services));
            })
            .platform_output
            .accesskit_update
            .unwrap()
    }

    fn click(&mut self, label: &str) -> TreeUpdate {
        let tree = self.frame(vec![]);
        let target = node(&tree, tr(label)).unwrap_or_else(|| panic!("Missing control: {label}"));
        self.frame(vec![action(Action::Click, target)])
    }

    fn wait_for(&mut self, label: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while node(&self.frame(vec![]), label).is_none() {
            assert!(Instant::now() < deadline, "Missing row: {label}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn node(tree: &TreeUpdate, label: &str) -> Option<NodeId> {
    tree.nodes
        .iter()
        .find_map(|(id, n)| (n.label() == Some(label)).then_some(*id))
}

fn action(action: Action, target: NodeId) -> Event {
    Event::AccessKitActionRequest(ActionRequest {
        action,
        target,
        data: None,
    })
}

fn with_library(
    respond: impl Fn(&url::Url) -> Value + Send + 'static,
    check: impl FnOnce(&mut Library<'_>),
) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(http_fixture::session(respond));
    let activity = server.listen_activity(|| {});
    let player = Player::new(|_| Err("Unexpected playback".into())).unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    check(&mut Library {
        ui: SignedUi::default(),
        ctx,
        services: Services {
            runtime: runtime.handle(),
            server: &server,
            activity: &activity,
            player: &player,
        },
    });
}

#[test]
fn switching_tabs_preserves_visible_rows_and_the_pending_next_page() {
    let (requested, requests) = mpsc::channel();
    let (finish, gate) = mpsc::channel();
    with_library(
        move |url| match url.path() {
            "/api/v1/library/albums" => {
                let next = url.query_pairs().any(|(k, v)| k == "after" && v == "two");
                requested.send(next).unwrap();
                if next {
                    gate.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                let name = if next { "Second album" } else { "First album" };
                json!({"items":[{"albumId":name,"name":name,"trackCount":1}],
                       "nextCursor":(!next).then_some("two")})
            }
            "/api/v1/library/tracks" => json!({"items":[
                {"trackId":"track","title":"Fixture track","artists":[],"albums":[]}
            ]}),
            _ => json!({"items":[]}),
        },
        |library| {
            let track = tr("播放 {0}").replace("{0}", "Fixture track");
            library.wait_for(&track);
            library.click("专辑");
            library.wait_for("First album");
            for next in [false, true] {
                assert_eq!(requests.recv_timeout(Duration::from_secs(5)).unwrap(), next);
            }
            assert!(node(&library.click("全部曲目"), &track).is_some());
            assert!(node(&library.click("专辑"), "First album").is_some());
            finish.send(()).unwrap();
            library.wait_for("Second album");
            assert!(node(&library.click("全部曲目"), &track).is_some());
            assert!(node(&library.click("专辑"), "Second album").is_some());
            assert!(requests.try_iter().next().is_none());
        },
    );
}

#[test]
fn album_queries_and_filters_are_isolated_and_manual_refresh_replaces_cached_rows() {
    let revisions = std::cell::RefCell::new(std::collections::HashMap::new());
    with_library(
        move |url| match url.path() {
            "/api/v1/library/albums" => {
                let pairs = url
                    .query_pairs()
                    .collect::<std::collections::HashMap<_, _>>();
                let query = pairs.get("q").map_or("all", |q| q.as_ref());
                let scope = if pairs.contains_key("unscrapedOnly") {
                    "unscraped"
                } else {
                    "any"
                };
                let key = format!("{query}-{scope}");
                let mut revisions = revisions.borrow_mut();
                let revision = revisions.entry(key.clone()).or_insert(0);
                *revision += 1;
                let name = format!("{key}-{revision}");
                json!({"items":[{"albumId":"album","name":name,"trackCount":1}]})
            }
            "/api/v1/library/artists" => {
                json!({"items":[{"artistId":"artist","name":"Only artist","trackCount":1}]})
            }
            _ => json!({"items":[]}),
        },
        |library| {
            library.click("专辑");
            library.wait_for("all-any-1");
            assert!(node(&library.click("仅显示未刮削专辑"), "all-any-1").is_none());
            library.wait_for("all-unscraped-1");
            let field = library
                .frame(vec![])
                .nodes
                .iter()
                .find(|(_, n)| n.role() == Role::TextInput)
                .unwrap()
                .0;
            library.frame(vec![action(Action::Focus, field)]);
            library.frame(vec![Event::Text("needle".into())]);
            let submitted = library.frame(vec![Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert!(node(&submitted, "all-unscraped-1").is_none());
            library.wait_for("needle-unscraped-1");
            assert!(node(&library.click("歌手"), "needle-unscraped-1").is_none());
            library.wait_for("Only artist");
            assert!(node(&library.click("专辑"), "all-unscraped-1").is_some());
            library.click("刷新分类");
            library.wait_for("all-unscraped-2");
            library.click("歌手");
            assert!(node(&library.click("专辑"), "all-unscraped-2").is_some());
            library.ui.navigate(Screen::Favorites);
            assert!(node(&library.frame(vec![]), "all-unscraped-2").is_none());
            library.ui.navigate(Screen::Library);
            assert!(node(&library.frame(vec![]), "all-unscraped-2").is_some());
        },
    );
}
