use super::QueueView;
use crate::i18n::tr;
use eframe::egui::{
    self,
    accesskit::{Action, ActionRequest, Role},
    Context, Event, FullOutput, Modifiers, Rect, ViewportEvent, ViewportId,
};
use musicfs_player_core::PlayerSnapshot;
use std::{cell::Cell, rc::Rc};

struct Harness {
    ctx: Context,
    view: QueueView,
    size: egui::Vec2,
    bar: Rect,
}

impl Harness {
    fn new() -> Self {
        let ctx = Context::default();
        ctx.enable_accesskit();
        ctx.style_mut(|style| style.animation_time = 0.);
        Self {
            ctx,
            view: QueueView::default(),
            size: egui::vec2(900., 650.),
            bar: Rect::NOTHING,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            events,
            ..Default::default()
        };
        self.ctx.run(input, |ctx| {
            let bar = egui::TopBottomPanel::bottom("player")
                .exact_height(60.)
                .show(ctx, |ui| ui.button(tr("播放队列")));
            self.bar = bar.response.rect;
            assert!(self
                .view
                .show(ctx, self.bar, &bar.inner, &PlayerSnapshot::default())
                .is_none());
        })
    }

    fn click(&mut self, label: &str) {
        // 新弹层的首帧用于测量，下一帧才可交互。
        self.frame(vec![]);
        let tree = self.frame(vec![]).platform_output.accesskit_update.unwrap();
        let (target, _) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == Role::Button && node.label() == Some(tr(label)))
            .unwrap_or_else(|| panic!("Missing queue button: {label}"));
        self.frame(vec![Event::AccessKitActionRequest(ActionRequest {
            action: Action::Click,
            target: *target,
            data: None,
        })]);
    }
}

#[test]
fn popup_tracks_the_bottom_right_corner_and_dismisses() {
    let mut ui = Harness::new();
    assert!(matches!(ui.view, QueueView::Hidden));
    ui.click("播放队列");
    for width in [900., 730.] {
        ui.size.x = width;
        let output = ui.frame(vec![]);
        assert!(matches!(ui.view, QueueView::Popup));
        assert_eq!(output.viewport_output.len(), 1);
        let popup = ui
            .ctx
            .memory(|memory| memory.area_rect(egui::Id::new("musicfs-queue-popup")))
            .expect("Queue popup in the root viewport");
        assert!((popup.right() - (ui.bar.right() - 8.)).abs() <= 1.);
        assert!((popup.bottom() - (ui.bar.top() - 8.)).abs() <= 1.);
    }

    let outside = egui::pos2(20., 20.);
    for pressed in [true, false] {
        ui.frame(vec![
            Event::PointerMoved(outside),
            Event::PointerButton {
                pos: outside,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]);
    }
    assert!(matches!(ui.view, QueueView::Hidden));
    ui.click("播放队列");
    ui.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    assert!(matches!(ui.view, QueueView::Hidden));
    ui.click("播放队列");
    ui.click("关闭");
    assert!(matches!(ui.view, QueueView::Hidden));
}

#[test]
fn pin_opens_a_child_viewport_and_native_close_resets_the_next_open() {
    let mut ui = Harness::new();
    ui.ctx.set_embed_viewports(false);
    let queue_id = ViewportId::from_hash_of("musicfs-queue-window");
    let close = Rc::new(Cell::new(false));
    let native_close = Rc::clone(&close);
    Context::set_immediate_viewport_renderer(move |ctx, viewport| {
        assert_eq!(viewport.ids.this, queue_id);
        assert_eq!(viewport.ids.parent, ViewportId::ROOT);
        let mut input = egui::RawInput {
            viewport_id: queue_id,
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 300.),
            )),
            ..Default::default()
        };
        let info = input.viewports.entry(queue_id).or_default();
        info.parent = Some(ViewportId::ROOT);
        if native_close.replace(false) {
            info.events.push(ViewportEvent::Close);
        }
        let _ = ctx.run(input, viewport.viewport_ui_cb);
    });
    ui.click("播放队列");
    assert!(!ui.frame(vec![]).viewport_output.contains_key(&queue_id));
    ui.click("固定播放队列");
    assert!(matches!(ui.view, QueueView::Pinned));
    assert!(ui.frame(vec![]).viewport_output.contains_key(&queue_id));
    close.set(true);
    ui.frame(vec![]);
    assert!(matches!(ui.view, QueueView::Hidden));
    ui.click("播放队列");
    assert!(matches!(ui.view, QueueView::Popup));
    assert!(!ui.frame(vec![]).viewport_output.contains_key(&queue_id));
}

#[test]
fn unpin_returns_to_popup_and_the_player_toggle_hides_a_pinned_queue() {
    let mut ui = Harness::new();
    ui.click("播放队列");
    ui.click("固定播放队列");
    ui.click("取消固定播放队列");
    assert!(matches!(ui.view, QueueView::Popup));
    ui.click("固定播放队列");
    ui.click("播放队列");
    assert!(matches!(ui.view, QueueView::Hidden));
    ui.click("播放队列");
    assert!(matches!(ui.view, QueueView::Popup));
}
