use eframe::egui::{self, Context, Pos2, Rect, Vec2};

const SNAP_DISTANCE: f32 = 12.;

#[derive(Clone, Copy, Debug)]
pub(super) struct HostGeometry {
    // 各窗口的 egui 点可能使用不同的缩放。窗口间的几何运算统一使用桌面物理像素。
    outer: Rect,
    client: Pos2,
    home: Pos2,
    scale: f32,
}

impl HostGeometry {
    pub(super) fn read(ctx: &Context, popup_corner: Pos2) -> Option<Self> {
        let scale = ctx.pixels_per_point();
        ctx.input(|i| {
            let info = i.viewport();
            let inner = info.inner_rect?;
            let outer = info.outer_rect.unwrap_or(inner) * scale;
            let client = inner.min * scale;
            Some(Self {
                outer,
                client,
                home: client + popup_corner.to_vec2() * scale,
                scale,
            })
        })
    }

    pub(super) fn popup_position(self, local_rect: Rect) -> Pos2 {
        self.client / self.scale + local_rect.min.to_vec2()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Axis {
    Before,
    Start,
    End,
    After,
    OffsetFromStart(f32),
    OffsetFromEnd(f32),
}

impl Axis {
    fn position(self, min: f32, max: f32, size: f32, scale: f32) -> f32 {
        match self {
            Self::Before => min - size,
            Self::Start => min,
            Self::End => max - size,
            Self::After => max,
            Self::OffsetFromStart(offset) => min + offset * scale,
            Self::OffsetFromEnd(offset) => max - size - offset * scale,
        }
    }

    fn snap(min: f32, max: f32, start: f32, size: f32, distance: f32) -> Option<Self> {
        [Self::Start, Self::End, Self::Before, Self::After]
            .into_iter()
            .map(|edge| (edge, (edge.position(min, max, size, 1.) - start).abs()))
            .filter(|(_, gap)| *gap <= distance)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(edge, _)| edge)
    }

    fn offset(min: f32, max: f32, start: f32, size: f32, scale: f32) -> Self {
        let leading = start - min;
        let trailing = max - (start + size);
        if leading.abs() <= trailing.abs() {
            Self::OffsetFromStart(leading / scale)
        } else {
            Self::OffsetFromEnd(trailing / scale)
        }
    }
}

#[derive(Default, Debug, PartialEq)]
enum Attachment {
    #[default]
    Home,
    Edge(Axis, Axis),
    Free,
}

impl Attachment {
    fn from_drop(host: HostGeometry, child: Rect) -> Self {
        let h = host.outer;
        let distance = SNAP_DISTANCE * host.scale;
        if (child.right_bottom() - host.home).abs().max_elem() <= distance {
            return Self::Home;
        }
        // 与主窗在两个方向均相邻时才吸附，避免吸向远处边缘的延长线。
        if !h.expand(distance).intersects(child) {
            return Self::Free;
        }
        let x = Axis::snap(h.left(), h.right(), child.left(), child.width(), distance);
        let y = Axis::snap(h.top(), h.bottom(), child.top(), child.height(), distance);
        if x.is_none() && y.is_none() {
            return Self::Free;
        }
        Self::Edge(
            x.unwrap_or_else(|| {
                Axis::offset(h.left(), h.right(), child.left(), child.width(), host.scale)
            }),
            y.unwrap_or_else(|| {
                Axis::offset(h.top(), h.bottom(), child.top(), child.height(), host.scale)
            }),
        )
    }

    fn position(&self, host: HostGeometry, size: Vec2) -> Option<Pos2> {
        let h = host.outer;
        match *self {
            Self::Home => Some(host.home - size),
            Self::Edge(x, y) => Some(egui::pos2(
                x.position(h.left(), h.right(), size.x, host.scale),
                y.position(h.top(), h.bottom(), size.y, host.scale),
            )),
            Self::Free => None,
        }
    }
}

#[derive(Default, Debug, PartialEq)]
pub(super) struct QueueDock {
    attachment: Attachment,
    dragging: bool,
    requested: Option<(Pos2, f32)>,
}

impl QueueDock {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn begin_drag(&mut self) {
        self.attachment = Attachment::Free;
        self.dragging = true;
        self.requested = None;
    }

    pub(super) fn update(&mut self, ctx: &Context, host: Option<HostGeometry>) -> Option<Pos2> {
        let host = host?;
        let scale = ctx.pixels_per_point();
        let (bounds, down) = ctx.input(|i| (i.viewport().outer_rect, i.pointer.primary_down()));
        let child = bounds? * scale;
        let target = self.target(host, child, down)?;
        // 实际停靠点已在屏幕上，无需再查询系统显示器。
        if (target - child.min).abs().max_elem() <= 1. {
            self.requested = None;
            return None;
        }
        self.move_if_needed(
            child.min,
            visible_position(target, child.size(), scale),
            scale,
        )
    }

    fn target(&mut self, host: HostGeometry, child: Rect, down: bool) -> Option<Pos2> {
        if self.dragging {
            if down {
                return None;
            }
            self.dragging = false;
            self.attachment = Attachment::from_drop(host, child);
        }
        self.attachment.position(host, child.size())
    }

    fn move_if_needed(&mut self, current: Pos2, target: Pos2, scale: f32) -> Option<Pos2> {
        if (target - current).abs().max_elem() <= 1. {
            self.requested = None;
            return None;
        }
        // 原生窗口稍后才会上报位置，跨屏 DPI 改变时必须用新的比例重新发送。
        if self.requested.is_some_and(|(last, old_scale)| {
            old_scale == scale && (last - target).abs().max_elem() <= 1.
        }) {
            return None;
        }
        self.requested = Some((target, scale));
        Some(target / scale)
    }
}

#[cfg(not(windows))]
fn visible_position(target: Pos2, _: Vec2, _: f32) -> Pos2 {
    target
}

#[cfg(windows)]
fn visible_position(target: Pos2, size: Vec2, scale: f32) -> Pos2 {
    use windows_sys::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    };
    // 主窗最大化或移动到桌面边缘时，至少保留队列标题栏可拖动。
    let title = RECT {
        left: (target.x + 20. * scale).round() as i32,
        right: (target.x + size.x - 20. * scale).round() as i32,
        top: (target.y + 4. * scale).round() as i32,
        bottom: (target.y + 26. * scale).round() as i32,
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let monitor = MonitorFromRect(&title, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() || GetMonitorInfoW(monitor, &mut info) == 0 {
            return target;
        }
    }
    let work = info.rcWork;
    if title.right > work.left
        && title.left < work.right
        && title.bottom > work.top
        && title.top < work.bottom
    {
        return target;
    }
    let x = target.x.clamp(
        work.left as f32,
        (work.right as f32 - size.x).max(work.left as f32),
    );
    let y = target.y.clamp(
        work.top as f32,
        (work.bottom as f32 - size.y).max(work.top as f32),
    );
    egui::pos2(x, y)
}

#[cfg(test)]
#[path = "../../../tests/native/queue_dock.rs"]
mod tests;
