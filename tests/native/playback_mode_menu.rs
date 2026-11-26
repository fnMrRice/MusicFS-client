use super::*;
use egui::{
    accesskit::{Node, Role, TreeUpdate},
    Event, Key, Modifiers, Pos2,
};

fn frame(ctx: &egui::Context, mode: &mut PlaybackMode, events: Vec<Event>) -> TreeUpdate {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1200., 800.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::TopBottomPanel::bottom("player").show(ctx, |ui| {
                if let Some(choice) = playback_mode_menu(ui, *mode) {
                    *mode = choice;
                }
            });
        },
    )
    .platform_output
    .accesskit_update
    .unwrap()
}

fn widget<'a>(tree: &'a TreeUpdate, role: Role, label: &str) -> &'a Node {
    tree.nodes
        .iter()
        .map(|(_, node)| node)
        .find(|node| node.role() == role && node.label() == Some(label))
        .expect("Expected a labeled playback control")
}

fn click(ctx: &egui::Context, mode: &mut PlaybackMode, node: &Node) -> TreeUpdate {
    let rect = node.bounds().unwrap();
    let pos = egui::pos2(
        (rect.x0 + rect.x1) as f32 / 2.,
        (rect.y0 + rect.y1) as f32 / 2.,
    );
    let pointer = |pressed| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        modifiers: Modifiers::NONE,
        pressed,
    };
    frame(ctx, mode, vec![Event::PointerMoved(pos), pointer(true)]);
    frame(ctx, mode, vec![pointer(false)]);
    frame(ctx, mode, vec![])
}

#[test]
fn clicking_each_playback_order_updates_the_button_and_closes_the_menu() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut mode = PlaybackMode::Sequence;
    let mut tree = frame(&ctx, &mut mode, vec![]);
    for (wanted, name) in PLAYBACK_MODES.into_iter().cycle().skip(1).take(4) {
        let button = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button)
            .unwrap();
        tree = click(&ctx, &mut mode, &button.1);
        assert_eq!(
            tree.nodes
                .iter()
                .filter(|(_, n)| n.role() == Role::RadioButton)
                .count(),
            4
        );
        tree = click(&ctx, &mut mode, widget(&tree, Role::RadioButton, tr(name)));
        assert_eq!(mode, wanted);
        widget(
            &tree,
            Role::Button,
            &tr("播放顺序：{0}").replace("{0}", tr(name)),
        );
        assert!(!tree
            .nodes
            .iter()
            .any(|(_, n)| n.role() == Role::RadioButton));
    }
}

#[test]
fn keyboard_can_open_and_change_playback_order() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut mode = PlaybackMode::Sequence;
    let tap = |key| {
        [true, false]
            .map(|pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
            .into()
    };
    frame(&ctx, &mut mode, vec![]);
    frame(&ctx, &mut mode, tap(Key::Tab));
    frame(&ctx, &mut mode, tap(Key::Enter));
    frame(&ctx, &mut mode, vec![]);
    frame(&ctx, &mut mode, tap(Key::Tab));
    frame(&ctx, &mut mode, tap(Key::Enter));
    assert_eq!(mode, PlaybackMode::RepeatOne);
}
