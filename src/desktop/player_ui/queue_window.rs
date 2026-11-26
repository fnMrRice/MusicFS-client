use super::{
    queue_dock::{HostGeometry, QueueDock},
    queue_panel::{self, PanelResponse},
};
use crate::{i18n::tr, ui::style};
use eframe::egui::{
    self, Align2, Context, Pos2, Rect, Response, Vec2, ViewportBuilder, ViewportClass,
    ViewportCommand, ViewportId,
};
use musicfs_player_core::{PlayerCommand, PlayerSnapshot};

const SIZE: Vec2 = egui::vec2(420., 384.);
const MIN_SIZE: Vec2 = egui::vec2(320., 180.);

#[derive(Default)]
pub(super) enum QueueView {
    #[default]
    Hidden,
    Popup,
    Pinned(PinnedQueue),
}

pub(super) struct PinnedQueue {
    popup_rect: Rect,
    initial_position: Option<Pos2>,
    dock: QueueDock,
}

fn panel_frame(ctx: &Context, popup: bool) -> egui::Frame {
    let visuals = &ctx.style().visuals;
    let frame = egui::Frame::new()
        .fill(style::surface(visuals.dark_mode))
        .stroke(egui::Stroke::new(
            1.0_f32,
            style::choice_border(visuals.dark_mode),
        ));
    if popup {
        frame.corner_radius(10).shadow(visuals.popup_shadow)
    } else {
        frame
    }
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
            *self = if matches!(self, Self::Hidden) {
                Self::Popup
            } else {
                Self::Hidden
            };
        }
        let bounds = ctx.screen_rect().with_max_y(bar.top()).shrink(8.);
        match self {
            Self::Hidden => None,
            Self::Popup => self.popup(ctx, bounds, toggle, snapshot),
            Self::Pinned(window) => {
                let host = HostGeometry::read(ctx, bounds.right_bottom());
                let response = window.show(ctx, host, snapshot)?;
                if response.close || response.toggle_pin {
                    *self = if response.close {
                        Self::Hidden
                    } else {
                        Self::Popup
                    };
                    if response.toggle_pin {
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                    ctx.request_repaint();
                }
                response.command
            }
        }
    }

    fn popup(
        &mut self,
        ctx: &Context,
        bounds: Rect,
        toggle: &Response,
        snapshot: &PlayerSnapshot,
    ) -> Option<PlayerCommand> {
        let size = SIZE.min(bounds.size());
        let frame = panel_frame(ctx, true);
        let popup = egui::Area::new(egui::Id::new("musicfs-queue-popup"))
            .kind(egui::UiKind::Popup)
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(bounds.right_bottom())
            .movable(false)
            .constrain_to(bounds)
            .default_size(size)
            .show(ctx, |ui| {
                frame
                    .show(ui, |ui| {
                        ui.set_min_size(size - frame.total_margin().sum());
                        ui.set_max_size(size - frame.total_margin().sum());
                        queue_panel::show(ui, snapshot, false)
                    })
                    .inner
            });
        if popup.inner.toggle_pin {
            let popup_rect = popup.response.rect;
            *self = Self::Pinned(PinnedQueue {
                popup_rect,
                initial_position: HostGeometry::read(ctx, bounds.right_bottom())
                    .map(|host| host.popup_position(popup_rect)),
                dock: QueueDock::new(),
            });
            ctx.request_repaint();
        } else if popup.inner.close
            || (popup.response.clicked_elsewhere() && toggle.clicked_elsewhere())
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            *self = Self::Hidden;
            ctx.request_repaint();
        }
        popup.inner.command
    }
}

impl PinnedQueue {
    fn show(
        &mut self,
        ctx: &Context,
        host: Option<HostGeometry>,
        snapshot: &PlayerSnapshot,
    ) -> Option<PanelResponse> {
        let id = ViewportId::from_hash_of("musicfs-queue-window");
        let title = tr("播放队列");
        let size = self.popup_rect.size();
        let mut builder = ViewportBuilder::default()
            .with_title(title)
            .with_decorations(false)
            .with_taskbar(false)
            .with_inner_size(size)
            .with_min_inner_size(MIN_SIZE.min(size))
            .with_always_on_top();
        // builder 仅保存首次位置。用户拖动后只在吸附目标改变时发定位命令。
        if let Some(pos) = self.initial_position {
            builder = builder.with_position(pos);
        }
        ctx.show_viewport_immediate(id, builder, |ctx, class| {
            if class == ViewportClass::Embedded {
                return egui::Window::new(title)
                    .id(id.0)
                    .title_bar(false)
                    .frame(panel_frame(ctx, true))
                    .default_rect(self.popup_rect)
                    .min_size(MIN_SIZE.min(size))
                    .show(ctx, |ui| queue_panel::show(ui, snapshot, true))
                    .and_then(|r| r.inner);
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                return Some(PanelResponse {
                    close: true,
                    ..Default::default()
                });
            }
            let response = egui::CentralPanel::default()
                .frame(panel_frame(ctx, false))
                .show(ctx, |ui| queue_panel::show(ui, snapshot, true))
                .inner;
            if response.start_drag {
                self.dock.begin_drag();
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if let Some(pos) = self.dock.update(ctx, host) {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
            }
            Some(response)
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/native/queue_window.rs"]
mod tests;
