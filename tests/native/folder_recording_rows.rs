use super::*;
use egui::accesskit::{Node, Role, TreeUpdate};

fn button<'a>(tree: &'a TreeUpdate, label: &str) -> Option<&'a Node> {
    tree.nodes
        .iter()
        .map(|(_, node)| node)
        .find(|node| node.role() == Role::Button && node.label() == Some(label))
}

fn click(node: &Node) -> Vec<egui::Event> {
    let bounds = node.bounds().unwrap();
    let pos = pos2(
        (bounds.x0 + bounds.x1) as f32 / 2.,
        (bounds.y0 + bounds.y1) as f32 / 2.,
    );
    let press = |pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    };
    vec![egui::Event::PointerMoved(pos), press(true), press(false)]
}

#[test]
fn folder_recording_controls_dispatch_only_their_source_row_at_each_width() {
    let tracks = [
        ("First", "available"),
        ("Missing", "unavailable"),
        ("Last", "available"),
    ]
    .map(|(title, state)| Recording {
        source_track_id: title.into(),
        provider_instance_id: "local".into(),
        display_name: format!("{title}.flac"),
        title: Some(title.into()),
        artists: None,
        album: None,
        cover_object_id: None,
        duration_ms: Some(8000),
        availability: state.into(),
    });
    for width in [340., 960.] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let frame = |events| {
            let mut action = None;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 360.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        action = list(ui, &tracks, &Providers::default());
                    });
                },
            );
            (output.platform_output.accesskit_update.unwrap(), action)
        };
        let (tree, _) = frame(vec![]);
        for (index, track) in tracks.iter().enumerate() {
            let title = track.title.as_ref().unwrap();
            let play_label = tr("播放 {0}").replace("{0}", title);
            let enqueue_label = format!("{} · {title}", tr("加入队列"));
            let play = button(&tree, &play_label).unwrap();
            let enqueue = button(&tree, &enqueue_label);
            let (_, action) = frame(click(play));
            if !playable(track) {
                assert!(play.is_disabled() && enqueue.is_none() && action.is_none());
                continue;
            }
            assert!(matches!(action, Some(Action::Play(i)) if i == index));
            let enqueue = enqueue.unwrap();
            assert!(play.bounds().unwrap().x1 <= enqueue.bounds().unwrap().x0);
            let (_, action) = frame(click(enqueue));
            assert!(matches!(action, Some(Action::Enqueue(i)) if i == index));
        }
    }
}
