use super::*;
use egui::{
    accesskit::{Action, ActionRequest, Role, TreeUpdate},
    Context, Event, Key, Modifiers,
};

fn frame(
    ctx: &Context,
    catalog: &mut Catalog,
    modifiers: Modifiers,
    events: Vec<Event>,
) -> TreeUpdate {
    let input = egui::RawInput {
        modifiers,
        events,
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| catalog.list_search(ui));
    })
    .platform_output
    .accesskit_update
    .unwrap()
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn find_focuses_the_list_search_enter_submits_and_clear_restores_all_tracks() {
    for shortcut in [Modifiers::CTRL, Modifiers::MAC_CMD] {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut catalog = Catalog::default();
        catalog.categories.started = true;
        catalog.tracks.started = true;
        let initial = frame(&ctx, &mut catalog, Modifiers::NONE, vec![]);
        let field = initial
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::TextInput)
            .unwrap()
            .0;
        assert_ne!(initial.focus, field);

        let modifier = Modifiers::COMMAND | shortcut;
        let focused = frame(&ctx, &mut catalog, modifier, vec![key(Key::F, modifier)]);
        assert_eq!(focused.focus, field);
        frame(
            &ctx,
            &mut catalog,
            Modifiers::NONE,
            vec![Event::Text("  音乐 demo  ".into())],
        );
        assert_eq!(catalog.list_draft, "  音乐 demo  ");
        assert!(catalog.list_query.is_empty());
        assert!(catalog.categories.started && catalog.tracks.started);

        let submitted = frame(
            &ctx,
            &mut catalog,
            Modifiers::NONE,
            vec![key(Key::Enter, Modifiers::NONE)],
        );
        assert_eq!(catalog.list_query, "音乐 demo");
        assert!(!catalog.categories.started && !catalog.tracks.started);
        let clear = submitted
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some(tr("清除")))
            .unwrap()
            .0;
        catalog.categories.started = true;
        catalog.tracks.started = true;
        frame(
            &ctx,
            &mut catalog,
            Modifiers::NONE,
            vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target: clear,
                data: None,
            })],
        );
        assert!(catalog.list_draft.is_empty() && catalog.list_query.is_empty());
        assert!(!catalog.categories.started && !catalog.tracks.started);
    }
}
