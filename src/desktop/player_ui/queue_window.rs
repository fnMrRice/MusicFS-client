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
    Window(QueueWindow),
}

pub(super) struct QueueWindow {
    pinned: bool,
    revealed: bool,
    #[cfg(windows)]
    was_foreground: bool,
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
        if matches!(self, Self::Hidden) {
            return None;
        }
        let bounds = ctx.screen_rect().with_max_y(bar.top()).shrink(8.);
        let size = SIZE.min(bounds.size());
        let popup_rect = Rect::from_min_size(bounds.max - size, size);
        let host = HostGeometry::read(ctx, bounds.right_bottom());
        let native_popup = cfg!(windows) && !ctx.embed_viewports();
        if matches!(self, Self::Popup) {
            if !native_popup || host.is_none() {
                return self.popup(ctx, bounds, toggle, snapshot);
            }
            *self = Self::Window(QueueWindow::new(popup_rect, host, false));
        }
        let Self::Window(window) = self else {
            return None;
        };
        if !window.pinned {
            let dismiss = ctx.input(|i| {
                i.viewport().minimized == Some(true)
                    || (i.pointer.any_pressed()
                        && i.pointer
                            .interact_pos()
                            .is_some_and(|p| !toggle.rect.contains(p)))
            });
            if dismiss || escape_pressed(ctx) {
                *self = Self::Hidden;
                return None;
            }
            window.popup_rect = popup_rect;
        }
        let response = window.show(ctx, host, snapshot)?;
        if response.close {
            *self = Self::Hidden;
        } else if response.toggle_pin {
            window.pinned = !window.pinned;
            if !window.pinned {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                if native_popup {
                    window.dock = QueueDock::new();
                    ctx.send_viewport_cmd_to(queue_id(), ViewportCommand::InnerSize(size));
                    ctx.send_viewport_cmd_to(queue_id(), ViewportCommand::Focus);
                } else {
                    *self = Self::Popup;
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
            }
        }
        if response.toggle_pin || response.close {
            ctx.request_repaint();
        }
        response.command
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
            *self = Self::Window(QueueWindow::new(
                popup_rect,
                HostGeometry::read(ctx, bounds.right_bottom()),
                true,
            ));
            ctx.request_repaint();
        } else if popup.inner.close
            || (popup.response.clicked_elsewhere() && toggle.clicked_elsewhere())
            || escape_pressed(ctx)
        {
            *self = Self::Hidden;
            ctx.request_repaint();
        }
        popup.inner.command
    }
}

fn escape_pressed(ctx: &Context) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

fn queue_id() -> ViewportId {
    ViewportId::from_hash_of("musicfs-queue-window")
}

impl QueueWindow {
    fn new(popup_rect: Rect, host: Option<HostGeometry>, pinned: bool) -> Self {
        Self {
            pinned,
            revealed: false,
            #[cfg(windows)]
            was_foreground: false,
            popup_rect,
            initial_position: host.map(|h| h.popup_position(popup_rect)),
            dock: QueueDock::new(),
        }
    }

    fn left_foreground(&mut self) -> bool {
        // 主窗与队列交接焦点时，两边都可能短暂报告失焦。只在前台明确切到其他应用后收起。
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, GetWindowThreadProcessId,
            };
            let foreground = unsafe { GetForegroundWindow() };
            let mut pid = 0;
            if foreground.is_null()
                || unsafe { GetWindowThreadProcessId(foreground, &mut pid) } == 0
            {
                return false;
            }
            if pid == std::process::id() {
                self.was_foreground = true;
                return false;
            }
            self.was_foreground
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    fn show(
        &mut self,
        ctx: &Context,
        host: Option<HostGeometry>,
        snapshot: &PlayerSnapshot,
    ) -> Option<PanelResponse> {
        let id = queue_id();

        let title = tr("播放队列");
        let size = self.popup_rect.size();
        let mut builder = ViewportBuilder::default()
            .with_title(title)
            .with_decorations(false)
            .with_taskbar(false)
            .with_inner_size(size)
            .with_min_inner_size(MIN_SIZE.min(size))
            .with_resizable(self.pinned)
            .with_window_level(if self.pinned {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            });
        if cfg!(windows) {
            builder = builder.with_active(false).with_visible(false);
        }
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
            if ctx.input(|i| i.viewport().close_requested())
                || (!self.pinned && (self.left_foreground() || escape_pressed(ctx)))
            {
                return Some(PanelResponse {
                    close: true,
                    ..Default::default()
                });
            }
            let response = egui::CentralPanel::default()
                .frame(panel_frame(ctx, false))
                .show(ctx, |ui| queue_panel::show(ui, snapshot, self.pinned))
                .inner;
            if response.start_drag {
                self.dock.begin_drag();
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if let Some(pos) = self.dock.update(ctx, host) {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
            }
            if cfg!(windows) && !self.revealed {
                // eframe 在首帧交换缓冲后执行命令，此时窗口已完成定位和绘制。
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                self.revealed = true;
            }
            Some(response)
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/native/queue_window.rs"]
mod tests;
