use super::*;
use crate::ui::{actions::Actions, http_fixture};
use egui::{
    accesskit::{Action, ActionRequest, Node, NodeId, Role, TreeUpdate},
    Context, Event,
};
use musicfs_player_core::Player;
use serde_json::json;

fn frame(
    ctx: &Context,
    catalog: &mut Catalog,
    svc: Services<'_>,
    events: Vec<Event>,
) -> TreeUpdate {
    let input = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200., 620.),
        )),
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(query) = catalog.library(ui, svc) {
                catalog.table_toolbar(ui, svc, &mut Actions::default(), &query, &[]);
            }
        });
    })
    .platform_output
    .accesskit_update
    .unwrap()
}

fn find<'a>(tree: &'a TreeUpdate, label: &str) -> Option<&'a (NodeId, Node)> {
    tree.nodes
        .iter()
        .find(|(_, n)| n.label() == Some(tr(label)))
}

fn click(id: NodeId) -> Vec<Event> {
    vec![Event::AccessKitActionRequest(ActionRequest {
        action: Action::Click,
        target: id,
        data: None,
    })]
}

fn bounds(tree: &TreeUpdate, label: &str) -> egui::accesskit::Rect {
    find(tree, label)
        .unwrap_or_else(|| panic!("Missing {label}"))
        .1
        .bounds()
        .unwrap()
}

fn search_box(tree: &TreeUpdate) -> egui::accesskit::Rect {
    tree.nodes
        .iter()
        .find(|(_, n)| n.role() == Role::TextInput)
        .unwrap()
        .1
        .bounds()
        .unwrap()
}

#[test]
fn catalog_toolbars_place_controls_and_open_each_management_dialog() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(http_fixture::session(|_| json!({"items":[]})));
    let activity = server.listen_activity(|| {});
    let player = Player::new(|_| Err("No playback requested".into())).unwrap();
    let svc = Services {
        runtime: rt.handle(),
        server: &server,
        activity: &activity,
        player: &player,
    };
    let actions = ["新建曲目", "一键合并重复音乐", "重复识别"];
    for action in actions {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut catalog = Catalog::default();
        let tree = frame(&ctx, &mut catalog, svc, vec![]);
        for hidden in ["仅可播放", "更多筛选"].into_iter().chain(actions) {
            assert!(find(&tree, hidden).is_none());
        }
        let more = bounds(&tree, "更多");
        let queue = bounds(&tree, "加入队列");
        let sort = bounds(&tree, "默认顺序");
        let search = search_box(&tree);
        assert!((0.0..16.).contains(&(more.x0 - queue.x1)));
        assert!(sort.x0 - more.x1 > 100.);
        assert!((0.0..16.).contains(&(search.x0 - sort.x1)));
        let tree = frame(
            &ctx,
            &mut catalog,
            svc,
            click(find(&tree, "更多").unwrap().0),
        );
        let id = find(&tree, action).expect("menu action").0;
        frame(&ctx, &mut catalog, svc, click(id));
        let tree = frame(&ctx, &mut catalog, svc, vec![]);
        let close = if action == "新建曲目" {
            "取消"
        } else {
            "关闭"
        };
        assert!(
            find(&tree, close).is_some(),
            "Dialog did not open: {action}"
        );
    }
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut catalog = Catalog {
        tabs: CategoryTab::Album,
        ..Default::default()
    };
    let tree = frame(&ctx, &mut catalog, svc, vec![]);
    let grid = bounds(&tree, "卡片视图");
    let list = bounds(&tree, "列表视图");
    let filter = bounds(&tree, "仅显示未刮削专辑");
    let scrape = bounds(&tree, "一键自动刮削");
    let search = search_box(&tree);
    assert!(grid.x1 <= list.x0 && list.x1 <= filter.x0);
    assert!(scrape.x0 - filter.x1 > 100.);
    assert!((0.0..16.).contains(&(search.x0 - scrape.x1)));
    for tab in [CategoryTab::Album, CategoryTab::Artist, CategoryTab::Genre] {
        catalog.open_credit(tab, "test".into(), "Test".into());
        let tree = frame(&ctx, &mut catalog, svc, vec![]);
        assert!(find(&tree, "播放全部").is_some());
        for hidden in actions {
            assert!(find(&tree, hidden).is_none());
        }
    }
}
