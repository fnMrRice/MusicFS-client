use super::*;
use egui::{Event, Key, Modifiers};

#[test]
fn enter_submits_the_focused_global_search_and_recents_keep_valid_queries() {
    let ctx = egui::Context::default();
    let mut focus = true;
    let mut search = LibrarySearch::default();
    let mut draw = |search: &mut LibrarySearch, events| {
        let mut submitted = false;
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| submitted |= search.form(ui, &mut focus));
        });
        submitted
    };
    assert!(!draw(&mut search, vec![]));
    assert!(!draw(&mut search, vec![Event::Text("  远山  ".into())]));
    assert!(draw(
        &mut search,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]
    ));
    assert_eq!(search.query, "远山");
    assert_eq!(search.recent, ["远山"]);

    search.draft = "歌".repeat(256);
    assert!(search.submit());
    search.draft.push('歌');
    assert!(!search.submit());
    assert_eq!(search.query.chars().count(), 256);
    search.draft = "远山".into();
    assert!(search.submit());
    search.clear();
    assert!(search.draft.is_empty() && search.query.is_empty());
    assert_eq!(search.recent.len(), 2);
    assert_eq!(search.recent[0], "远山");
}
