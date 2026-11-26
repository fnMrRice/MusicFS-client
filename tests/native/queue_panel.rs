use super::*;
use eframe::egui::{
    accesskit::{Node, Role, TreeUpdate},
    Event, Modifiers, PointerButton, Pos2, Rect,
};
use musicfs_player_core::Track;

fn tracks() -> PlayerSnapshot {
    let track: Track = serde_json::from_value(serde_json::json!({
        "id":"track","title":"Wings of Piano","artists":["V.K克"],"durationMs":168000
    }))
    .unwrap();
    PlayerSnapshot {
        queue: vec![track; 896].into(),
        index: Some(5),
        status: musicfs_player_core::Status::Playing,
        ..Default::default()
    }
}

fn frame(
    ctx: &egui::Context,
    s: &PlayerSnapshot,
    events: Vec<Event>,
    width: f32,
) -> (PanelResponse, TreeUpdate) {
    let mut result = PanelResponse::default();
    let size = egui::vec2(width, 384.);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    result = show(ui, s, true);
                    assert!(ui.min_rect().width() <= width + 0.5);
                });
        },
    );
    (result, output.platform_output.accesskit_update.unwrap())
}

fn node<'a>(tree: &'a TreeUpdate, label: &str, role: Role) -> &'a Node {
    tree.nodes
        .iter()
        .map(|(_, n)| n)
        .find(|n| n.role() == role && (n.label() == Some(label) || n.value() == Some(label)))
        .unwrap_or_else(|| panic!("Missing queue control: {label}"))
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn queue_header_buttons_share_one_row_and_clear_has_its_own_hit_target() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let snapshot = tracks();
    frame(&ctx, &snapshot, vec![], 320.);
    let (_, tree) = frame(&ctx, &snapshot, vec![], 320.);
    let clear = node(&tree, tr("清空队列"), Role::Button).bounds().unwrap();
    let pin = node(&tree, tr("取消固定播放队列"), Role::Button)
        .bounds()
        .unwrap();
    let close = node(&tree, tr("关闭"), Role::Button).bounds().unwrap();
    let count = node(&tree, &tr("{0} 首曲目").replace("{0}", "896"), Role::Label)
        .bounds()
        .unwrap();
    assert!(count.x1 <= clear.x0 && clear.x1 <= pin.x0 && pin.x1 <= close.x0);
    assert!(count.y0 < clear.y1 && clear.y0 < count.y1);
    assert!(clear.y0 < 32. && pin.y0 < 32. && close.y0 < 32.);
    let p = egui::pos2(
        ((clear.x0 + clear.x1) / 2.) as f32,
        ((clear.y0 + clear.y1) / 2.) as f32,
    );
    let (press, _) = frame(
        &ctx,
        &snapshot,
        vec![Event::PointerMoved(p), button(p, true)],
        320.,
    );
    assert!(!press.start_drag);
    let (release, _) = frame(&ctx, &snapshot, vec![button(p, false)], 320.);
    assert!(matches!(release.command, Some(PlayerCommand::Clear)));
    let (_, empty) = frame(&ctx, &PlayerSnapshot::default(), vec![], 320.);
    assert!(node(&empty, tr("清空队列"), Role::Button).is_disabled());
}

#[test]
fn pinned_header_drags_from_title_count_and_free_space() {
    for x in [20., 102., 178.] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let q = tracks();
        frame(&ctx, &q, vec![], 420.);
        frame(&ctx, &q, vec![], 420.);
        let at = egui::pos2(x, 25.);
        let (press, _) = frame(
            &ctx,
            &q,
            vec![Event::PointerMoved(at), button(at, true)],
            420.,
        );
        let (move_, _) = frame(
            &ctx,
            &q,
            vec![Event::PointerMoved(at + egui::vec2(18., 2.))],
            420.,
        );
        assert!(
            press.start_drag || move_.start_drag,
            "Header position {x} did not start a drag"
        );
    }
}

#[test]
fn the_entire_track_row_selects_its_queue_entry() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let q = tracks();
    frame(&ctx, &q, vec![], 420.);
    frame(&ctx, &q, vec![], 420.);
    let p = egui::pos2(396., 57. + 47. + 1. + 23.);
    let (down, _) = frame(
        &ctx,
        &q,
        vec![Event::PointerMoved(p), button(p, true)],
        420.,
    );
    assert!(!down.start_drag);
    let (up, _) = frame(&ctx, &q, vec![button(p, false)], 420.);
    assert!(matches!(
        up.command,
        Some(PlayerCommand::Select { index: 1 })
    ));
}

#[test]
fn scroll_position_survives_pinning_and_unpinning() {
    let ctx = egui::Context::default();
    let q = tracks();
    let pinned = egui::ViewportId::from_hash_of("native-queue");
    let draw = |viewport: egui::ViewportId, offset: Option<f32>| {
        let mut scroll = 0.;
        let mut input = egui::RawInput {
            viewport_id: viewport,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(420., 384.))),
            ..Default::default()
        };
        input.viewports.entry(viewport).or_default();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    ui.push_id(viewport, |ui| {
                        let id = ui.make_persistent_id(egui::Id::new("queue-tracks"));
                        if let Some(y) = offset {
                            let mut state =
                                egui::scroll_area::State::load(ctx, id).unwrap_or_default();
                            state.offset.y = y;
                            state.store(ctx, id);
                        }
                        show(ui, &q, viewport == pinned);
                        scroll = egui::scroll_area::State::load(ctx, id).unwrap().offset.y;
                    });
                });
        });
        scroll
    };
    draw(egui::ViewportId::ROOT, None);
    let before = draw(egui::ViewportId::ROOT, Some(14496.));
    assert_eq!(before, 14496.);
    assert_eq!(draw(pinned, None), before);
    let after = draw(pinned, Some(15840.));
    assert_eq!(after, 15840.);
    assert_eq!(draw(egui::ViewportId::ROOT, None), after);
}
