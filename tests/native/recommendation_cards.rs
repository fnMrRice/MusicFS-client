use super::*;
use egui::{accesskit::Role, Event, Key, Modifiers, Pos2, Vec2};
use serde_json::json;

struct Page {
    ctx: egui::Context,
    source: Source,
}

impl Page {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        Self {
            ctx,
            source: serde_json::from_value(json!({
                "instanceId": "test-source", "stale": false,
                "items": [{"playlist": {"title": "Daily songs", "externalId": "sample", "coverUrl": "cover"}, "reason": ""}]
            })).unwrap(),
        }
    }

    fn frame(&self, events: Vec<Event>) -> (Rect, Rect, bool) {
        let mut cover = Rect::NOTHING;
        let mut selected = None;
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600., 300.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    selected = show(ui, &self.source, |_, rect, url| {
                        assert_eq!(url, Some("cover"));
                        cover = rect;
                    });
                });
            },
        );
        let tree = output.platform_output.accesskit_update.unwrap();
        let b = tree
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.role() == Role::Button && node.label() == Some("Daily songs"))
                    .then(|| node.bounds().unwrap())
            })
            .expect("The whole card must expose one named button");
        let bounds = Rect::from_min_max(
            egui::pos2(b.x0 as f32, b.y0 as f32),
            egui::pos2(b.x1 as f32, b.y1 as f32),
        );
        if let Some(detail) = &selected {
            assert_eq!(detail.key(), ("test-source", "sample"));
        }
        (bounds, cover, selected.is_some())
    }
}

#[test]
fn cover_title_and_empty_card_padding_open_the_same_playlist() {
    let page = Page::new();
    let (card, cover, _) = page.frame(vec![]);
    for pos in [
        cover.center(),
        egui::pos2(cover.right() + 22., card.center().y),
        card.right_bottom() - Vec2::splat(3.),
    ] {
        let pointer = |pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        page.frame(vec![Event::PointerMoved(pos), pointer(true)]);
        assert!(
            page.frame(vec![pointer(false)]).2,
            "Click missed the card at {pos:?}"
        );
    }
}

#[test]
fn keyboard_focus_opens_the_recommendation_card() {
    let page = Page::new();
    page.frame(vec![]);
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
    page.frame(tap(Key::Tab));
    assert!(page.frame(tap(Key::Enter)).2);
}
