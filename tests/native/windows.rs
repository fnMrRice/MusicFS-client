use super::*;
use egui::accesskit::{Action, ActionRequest, Role};

fn idle() -> Presentation {
    Presentation {
        instance: 1,
        generation: 0,
        title: None,
        artists: vec![],
        source_id: None,
        status: Status::Idle,
        position_ms: 0,
        duration_ms: None,
        has_next: false,
        has_previous: false,
        error: None,
    }
}

#[test]
fn restore_button_brings_back_main_and_closes_its_own_viewport() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(false);
    let mini_id = ViewportId::from_hash_of("musicfs-mini");
    let visible = AtomicBool::new(true);
    let now = idle();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        ctx.show_viewport_deferred(mini_id, ViewportBuilder::default(), |_, _| {});
    });
    let frame = |events| {
        let mut input = egui::RawInput {
            viewport_id: mini_id,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(410., 146.),
            )),
            events,
            ..Default::default()
        };
        input.viewports.insert(
            mini_id,
            egui::ViewportInfo {
                parent: Some(ViewportId::ROOT),
                ..Default::default()
            },
        );
        ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| mini_header(ui, &now, &visible));
        })
    };
    let tree = frame(vec![]).platform_output.accesskit_update.unwrap();
    let (id, _) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Button && node.label() == Some("主窗口"))
        .expect("main window button");
    let output = frame(vec![egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::Click,
        target: *id,
        data: None,
    })]);
    let main = &output.viewport_output[&ViewportId::ROOT].commands;
    let mini = &output.viewport_output[&mini_id].commands;
    assert!(!visible.load(Ordering::Relaxed));
    assert!(main.contains(&ViewportCommand::Minimized(false)));
    assert!(main.contains(&ViewportCommand::Focus));
    assert!(mini.contains(&ViewportCommand::Close));
}

#[test]
fn lyrics_header_finds_lyrics_without_leaving_the_window() {
    let ctx = Context::default();
    ctx.enable_accesskit();
    let mut now = idle();
    now.source_id = Some("track-a".into());
    let mut searching = false;
    let mut frame = |events| {
        ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if lyrics_header(ui, &now, searching, false) {
                        searching = !searching;
                    }
                });
            },
        )
    };
    let tree = frame(vec![]).platform_output.accesskit_update.unwrap();
    let entry = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Button && node.label() == Some(tr("查找歌词")))
        .expect("find lyrics button")
        .0;
    assert!(!tree
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some(tr("主窗口"))));
    let output = frame(vec![egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::Click,
        target: entry,
        data: None,
    })]);
    assert!(!output.viewport_output[&ViewportId::ROOT]
        .commands
        .contains(&ViewportCommand::Close));
    let tree = frame(vec![]).platform_output.accesskit_update.unwrap();
    assert!(tree
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some(tr("返回歌词"))));
}
