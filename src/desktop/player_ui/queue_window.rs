use super::queue_list;
use crate::{i18n::tr, icons};
use eframe::egui::{
    self, Align, Align2, Context, Layout, Rect, Response, Ui, Vec2, ViewportBuilder, ViewportClass,
    ViewportCommand, ViewportId,
};
use musicfs_player_core::{PlayerCommand, PlayerSnapshot};

const SIZE: Vec2 = egui::vec2(390., 305.);
const MIN_SIZE: Vec2 = egui::vec2(255., 170.);

#[derive(Default, Debug, PartialEq, Eq)]
pub(super) enum QueueView {
    #[default]
    Hidden,
    Popup,
    Pinned,
}

impl QueueView {
    pub(super) fn show(
        &mut self,
        ctx: &Context,
        bar: Rect,
        toggle: &Response,
        snapshot: &PlayerSnapshot,
    ) -> Option<PlayerCommand> {
        if toggle.clicked() {
            *self = if *self == Self::Hidden {
                Self::Popup
            } else {
                Self::Hidden
            };
        }
        match self {
            Self::Hidden => None,
            Self::Popup => self.popup(ctx, bar, toggle, snapshot),
            Self::Pinned => self.window(ctx, snapshot),
        }
    }

    fn popup(
        &mut self,
        ctx: &Context,
        bar: Rect,
        toggle: &Response,
        snapshot: &PlayerSnapshot,
    ) -> Option<PlayerCommand> {
        let bounds = ctx.screen_rect().with_max_y(bar.top()).shrink(8.);
        let size = SIZE.min(bounds.size());
        let frame = egui::Frame::popup(&ctx.style());
        let popup = egui::Area::new(egui::Id::new("musicfs-queue-popup"))
            .kind(egui::UiKind::Popup)
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(bounds.right_bottom())
            .constrain_to(bounds)
            .default_size(size)
            .show(ctx, |ui| {
                frame
                    .show(ui, |ui| {
                        let inner = size - frame.total_margin().sum();
                        ui.set_width(inner.x);
                        ui.set_height(inner.y);
                        self.contents(ui, snapshot)
                    })
                    .inner
            });
        if (popup.response.clicked_elsewhere() && toggle.clicked_elsewhere())
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            *self = Self::Hidden;
        }
        popup.inner
    }

    fn window(&mut self, ctx: &Context, snapshot: &PlayerSnapshot) -> Option<PlayerCommand> {
        let id = ViewportId::from_hash_of("musicfs-queue-window");
        let title = tr("播放队列");
        let builder = ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(SIZE)
            .with_min_inner_size(MIN_SIZE)
            .with_always_on_top();
        let command = ctx.show_viewport_immediate(id, builder, |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut open = true;
                let window = egui::Window::new(title)
                    .id(id.0)
                    .open(&mut open)
                    .collapsible(false)
                    .default_size(SIZE)
                    .min_size(MIN_SIZE)
                    .show(ctx, |ui| self.contents(ui, snapshot));
                if !open {
                    *self = Self::Hidden;
                }
                return window.and_then(|r| r.inner).flatten();
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                *self = Self::Hidden;
                return None;
            }
            egui::CentralPanel::default()
                .show(ctx, |ui| self.contents(ui, snapshot))
                .inner
        });
        if *self != Self::Pinned {
            if *self == Self::Popup {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            ctx.request_repaint();
        }
        command
    }

    fn contents(&mut self, ui: &mut Ui, snapshot: &PlayerSnapshot) -> Option<PlayerCommand> {
        ui.horizontal(|ui| {
            ui.strong(tr("播放队列"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, "close", tr("关闭"), 28., 16.).clicked() {
                    *self = Self::Hidden;
                }
                let pinned = *self == Self::Pinned;
                let label = if pinned {
                    tr("取消固定播放队列")
                } else {
                    tr("固定播放队列")
                };
                if icons::button(ui, "pin", label, 28., 16.).clicked() {
                    *self = if pinned { Self::Popup } else { Self::Pinned };
                }
            });
        });
        queue_list::show(ui, snapshot)
    }
}

#[cfg(test)]
#[path = "../../../tests/native/queue_window.rs"]
mod tests;
