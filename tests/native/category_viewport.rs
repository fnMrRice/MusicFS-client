use super::*;
use egui::{Context, Id, Pos2, Vec2};

#[derive(Default)]
struct Frame {
    visits: usize,
    visible: Vec<(usize, Id, Rect)>,
    max_scroll: f32,
    more_visible: bool,
    clicked: Option<usize>,
    focused: Option<usize>,
}

fn frame(ctx: &Context, cols: usize, offset: Option<f32>, events: Vec<egui::Event>) -> Frame {
    let mut result = Frame::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(870., 480.))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut scroll = egui::ScrollArea::vertical();
                if let Some(offset) = offset {
                    scroll = scroll.vertical_scroll_offset(offset);
                }
                let output = scroll.show(ui, |ui| {
                    if cols == 1 {
                        ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 32.),
                            Sense::hover(),
                        );
                    }
                    let card_width = (ui.available_width()
                        - (cols - 1) as f32 * ui.spacing().item_spacing.x)
                        / cols as f32;
                    let size = Vec2::new(card_width, if cols == 1 { 55. } else { 68. });
                    category_cards(ui, 10_001, cols, size, |ui, index, rect, response| {
                        result.visits += 1;
                        if response.has_focus() {
                            result.focused = Some(index);
                        }
                        if ui.is_rect_visible(rect) {
                            result.visible.push((index, response.id, rect));
                        }
                        if response.clicked() {
                            result.clicked = Some(index);
                        }
                    });
                    let more = ui.button("More");
                    result.more_visible = ui.is_rect_visible(more.rect);
                });
                result.max_scroll = output.content_size.y - output.inner_rect.height();
            });
        },
    );
    result
}

#[test]
fn category_layout_visits_only_viewport_cards_and_keeps_the_last_item_selectable() {
    for cols in [1, 4] {
        let ctx = Context::default();
        let top = frame(&ctx, cols, None, vec![]);
        let middle = frame(&ctx, cols, Some(top.max_scroll / 2.), vec![]);
        let bottom = frame(&ctx, cols, Some(top.max_scroll), vec![]);
        for snapshot in [&top, &middle, &bottom] {
            assert!(
                snapshot.visits <= 12 * cols,
                "visited {} cards with {cols} columns",
                snapshot.visits
            );
        }
        assert_eq!(top.visible.first().unwrap().0, 0);
        assert!(!top.more_visible && !middle.more_visible && bottom.more_visible);
        assert!((4_900..5_100).contains(&middle.visible[0].0));
        let &(index, id, rect) = bottom.visible.last().unwrap();
        assert_eq!(index, 10_000);
        let pointer = |pressed| egui::Event::PointerButton {
            pos: rect.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let pressed = frame(
            &ctx,
            cols,
            Some(top.max_scroll),
            vec![egui::Event::PointerMoved(rect.center()), pointer(true)],
        );
        assert_eq!(pressed.visible.last().unwrap().1, id);
        let released = frame(&ctx, cols, Some(top.max_scroll), vec![pointer(false)]);
        assert_eq!(released.clicked, Some(index));
    }
}

#[test]
fn tab_and_shift_tab_scroll_across_the_visible_category_rows() {
    let ctx = Context::default();
    ctx.style_mut(|style| style.scroll_animation = egui::style::ScrollAnimation::none());
    let top = frame(&ctx, 1, None, vec![]);
    let &(last, id, _) = top.visible.last().unwrap();
    ctx.memory_mut(|memory| memory.request_focus(id));
    frame(&ctx, 1, None, vec![]);
    let next = (last + 1..=last + 12).map(|index| (index, false));
    let back = (0..last + 12).rev().map(|index| (index, true));
    for (index, shift) in next.chain(back) {
        let tab = |pressed| egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        };
        frame(&ctx, 1, None, vec![tab(true)]);
        frame(&ctx, 1, None, vec![tab(false)]);
        // egui 在一帧结束时提交滚动目标，在后续帧更新视口。
        frame(&ctx, 1, None, vec![]);
        let after = frame(&ctx, 1, None, vec![]);
        assert_eq!(after.focused, Some(index));
        assert!(after.visible.iter().any(|&(item, _, _)| item == index));
    }
}
