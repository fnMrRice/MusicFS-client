use super::QueueView;
use crate::i18n::tr;
use eframe::egui::{
    self,
    accesskit::{Action, ActionRequest, Role},
    Context, Event, FullOutput, Modifiers, Rect, ViewportId,
};
use musicfs_player_core::PlayerSnapshot;

struct Harness {
    ctx: Context,
    view: QueueView,
    size: egui::Vec2,
    bar: Rect,
    focused: bool,
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
            focused: true,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            events,
            focused: self.focused,
            ..Default::default()
        };
        let host = input.viewports.entry(ViewportId::ROOT).or_default();
        host.inner_rect = Some(Rect::from_min_size(egui::pos2(140., 70.), self.size));
        host.focused = Some(self.focused);
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

    fn click(&mut self, label: &str) -> FullOutput {
        // 新弹层的首帧用于测量，下一帧才可交互。
        self.frame(vec![]);
        let click = click_event(&self.frame(vec![]), label);
        self.frame(vec![click])
    }
}

fn click_event(output: &FullOutput, label: &str) -> Event {
    let tree = output.platform_output.accesskit_update.as_ref().unwrap();
    let (target, _) = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Button && node.label() == Some(tr(label)))
        .unwrap_or_else(|| panic!("Missing queue button: {label}"));
    Event::AccessKitActionRequest(ActionRequest {
        action: Action::Click,
        target: *target,
        data: None,
    })
}

fn escape() -> Event {
    Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
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
    ui.frame(vec![escape()]);
    assert!(matches!(ui.view, QueueView::Hidden));
    ui.click("播放队列");
    ui.click("关闭");
    assert!(matches!(ui.view, QueueView::Hidden));
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

mod native {
    use super::super::{queue_id, SIZE};
    use super::*;
    use egui::{ViewportBuilder, ViewportCommand, ViewportEvent};
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct ChildFrame {
        events: Vec<Event>,
        close_requested: bool,
        focused: bool,
        output: Option<FullOutput>,
        builder: Option<ViewportBuilder>,
    }

    fn attach_renderer(ui: &Harness) -> Rc<RefCell<ChildFrame>> {
        ui.ctx.set_embed_viewports(false);
        let child = Rc::new(RefCell::new(ChildFrame {
            focused: true,
            ..Default::default()
        }));
        let render = Rc::clone(&child);
        Context::set_immediate_viewport_renderer(move |ctx, viewport| {
            assert_eq!(viewport.ids.this, queue_id());
            assert_eq!(viewport.ids.parent, ViewportId::ROOT);
            let builder = viewport.builder;
            let initial_flag = cfg!(windows).then_some(false);
            assert_eq!(builder.visible, initial_flag);
            assert_eq!(builder.active, initial_flag);
            assert_eq!(builder.decorations, Some(false));
            let size = SIZE;
            let position = egui::pos2(140. + 900. - 8. - size.x, 70. + 650. - 60. - 8. - size.y);

            let mut child = render.borrow_mut();
            if let Some(previous) = &mut child.builder {
                // eframe 用这个返回值决定是否销毁并重建原生窗口。
                let (changes, recreate) = previous.patch(builder);
                assert!(!recreate);
                assert!(!changes
                    .iter()
                    .any(|cmd| matches!(cmd, ViewportCommand::Visible(_))));
            } else {
                assert_eq!(builder.inner_size, Some(size));
                assert!(builder.position.unwrap().distance(position) <= 1.);
                child.builder = Some(builder);
            }
            let mut input = egui::RawInput {
                viewport_id: queue_id(),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                events: std::mem::take(&mut child.events),
                focused: child.focused,
                ..Default::default()
            };
            let info = input.viewports.entry(queue_id()).or_default();
            info.parent = Some(ViewportId::ROOT);
            info.outer_rect = Some(Rect::from_min_size(position, size));
            info.inner_rect = info.outer_rect;
            info.focused = Some(child.focused);
            if std::mem::take(&mut child.close_requested) {
                info.events.push(ViewportEvent::Close);
            }
            child.output = Some(ctx.run(input, viewport.viewport_ui_cb));
        });
        child
    }

    fn click_child(child: &RefCell<ChildFrame>, label: &str) {
        let mut child = child.borrow_mut();
        let click = click_event(child.output.as_ref().unwrap(), label);
        child.events.push(click);
    }

    fn assert_visible_frame(root: &FullOutput, first: bool) {
        assert_eq!(root.viewport_output.len(), 2);
        let output = &root.viewport_output[&queue_id()];
        assert_eq!(output.parent, ViewportId::ROOT);
        let visibility: Vec<_> = output
            .commands
            .iter()
            .filter_map(|command| match command {
                ViewportCommand::Visible(value) => Some(*value),
                ViewportCommand::Close => panic!("Queue closed while switching mode"),
                _ => None,
            })
            .collect();
        let reveal = first && cfg!(windows);
        assert_eq!(
            visibility.as_slice(),
            if reveal { &[true][..] } else { &[] }
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_popup_pins_and_unpins_in_place_after_a_single_reveal() {
        let mut ui = Harness::new();
        let child = attach_renderer(&ui);
        assert_visible_frame(&ui.click("播放队列"), true);
        for label in ["固定播放队列", "取消固定播放队列", "固定播放队列"] {
            assert_visible_frame(&ui.frame(vec![]), false);
            click_child(&child, label);
            assert_visible_frame(&ui.frame(vec![]), false);
        }
        // 从已固定状态原生关闭，再打开时回到弹出模式。
        child.borrow_mut().close_requested = true;
        ui.frame(vec![]);
        assert!(matches!(ui.view, QueueView::Hidden));
        child.borrow_mut().builder = None;
        assert_visible_frame(&ui.click("播放队列"), true);
        assert_visible_frame(&ui.frame(vec![]), false);
        click_child(&child, "固定播放队列");
        assert_visible_frame(&ui.frame(vec![]), false);
    }

    #[cfg(windows)]
    #[test]
    fn unpinned_window_dismisses_with_escape_and_reopened_popup_with_header_close() {
        let mut ui = Harness::new();
        let child = attach_renderer(&ui);
        assert_visible_frame(&ui.click("播放队列"), true);
        for label in ["固定播放队列", "取消固定播放队列"] {
            assert_visible_frame(&ui.frame(vec![]), false);
            click_child(&child, label);
            assert_visible_frame(&ui.frame(vec![]), false);
        }
        child.borrow_mut().events.push(escape());
        ui.frame(vec![]);
        assert!(matches!(ui.view, QueueView::Hidden));
        child.borrow_mut().builder = None;
        assert_visible_frame(&ui.click("播放队列"), true);
        ui.frame(vec![]);
        click_child(&child, "关闭");
        ui.frame(vec![]);
        assert!(matches!(ui.view, QueueView::Hidden));
    }

    #[cfg(windows)]
    #[test]
    fn native_popup_survives_the_focus_handoff_from_player_to_queue() {
        let mut ui = Harness::new();
        let child = attach_renderer(&ui);
        child.borrow_mut().focused = false;
        assert_visible_frame(&ui.click("播放队列"), true);
        // Windows 可以先上报主窗失焦，随后才把焦点交给队列。
        ui.focused = false;
        assert_visible_frame(&ui.frame(vec![]), false);
        child.borrow_mut().focused = true;
        assert_visible_frame(&ui.frame(vec![]), false);
        click_child(&child, "固定播放队列");
        assert_visible_frame(&ui.frame(vec![]), false);
        assert_visible_frame(&ui.frame(vec![]), false);
        click_child(&child, "取消固定播放队列");
        assert_visible_frame(&ui.frame(vec![]), false);
    }

    #[cfg(not(windows))]
    #[test]
    fn native_queue_opens_only_when_pinned_and_close_restores_embedded_popup() {
        let mut ui = Harness::new();
        let child = attach_renderer(&ui);
        assert_eq!(ui.click("播放队列").viewport_output.len(), 1);
        ui.click("固定播放队列");
        assert_visible_frame(&ui.frame(vec![]), true);
        assert_visible_frame(&ui.frame(vec![]), false);
        click_child(&child, "取消固定播放队列");
        ui.frame(vec![]);
        assert!(matches!(ui.view, QueueView::Popup));
        child.borrow_mut().builder = None;
        ui.click("固定播放队列");
        assert_visible_frame(&ui.frame(vec![]), true);
        child.borrow_mut().close_requested = true;
        ui.frame(vec![]);
        assert!(matches!(ui.view, QueueView::Hidden));
        assert_eq!(ui.click("播放队列").viewport_output.len(), 1);
    }
}
