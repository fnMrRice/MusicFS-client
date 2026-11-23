use super::{
    widgets::{credit, title_control},
    *,
};
use eframe::egui::{
    accesskit::{Action, ActionRequest, Role, TreeUpdate},
    Context, Event, Rect,
};
use musicfs_server_adapter::TrackCredit;

#[derive(Default)]
struct Clicks {
    play: bool,
    artist: Option<String>,
    disclosure: bool,
}

fn frame(ctx: &Context, events: Vec<Event>, enabled: bool) -> (TreeUpdate, Clicks) {
    let mut clicks = Clicks::default();
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = |y: f32| Rect::from_min_size(egui::pos2(20., y), egui::vec2(210., 24.));
            clicks.play =
                title_control(ui, rect(25.), "play", "Track", "Play Track", enabled, false)
                    .clicked();
            clicks.disclosure = title_control(
                ui,
                rect(56.),
                "chevron",
                "2 sources",
                "Expand sources",
                true,
                true,
            )
            .clicked();
            let credit_event = credit(
                ui,
                rect(90.),
                &[TrackCredit {
                    id: "artist-42".into(),
                    name: "Artist".into(),
                }],
                "Artist",
                true,
                CategoryTab::Artist,
            );
            if let Some(RowEvent::Credit(CategoryTab::Artist, id, _)) = credit_event {
                clicks.artist = Some(id);
            }
        });
    });
    (output.platform_output.accesskit_update.unwrap(), clicks)
}

#[test]
fn keyboard_accessible_row_controls_dispatch_only_their_own_action() {
    for (label, role) in [
        ("Play Track", Role::Button),
        ("Expand sources", Role::Button),
        ("Artist", Role::Link),
    ] {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let (tree, _) = frame(&ctx, vec![], true);
        let (id, _) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == role && n.label() == Some(label))
            .unwrap();
        let (_, clicked) = frame(
            &ctx,
            vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target: *id,
                data: None,
            })],
            true,
        );
        assert_eq!(clicked.play, label == "Play Track");
        assert_eq!(clicked.disclosure, label == "Expand sources");
        assert_eq!(
            clicked.artist.as_deref(),
            (label == "Artist").then_some("artist-42")
        );
    }
}
