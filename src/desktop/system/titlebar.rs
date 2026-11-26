use eframe::egui::{Context, Rect};

#[cfg(windows)]
pub(crate) use platform::{has_custom_titlebar, maximize_hovered, publish_titlebar};
#[cfg(windows)]
pub(super) use platform::{register, Regions};

#[cfg(not(windows))]
pub(crate) fn has_custom_titlebar(_: &Context) -> bool {
    false
}
#[cfg(not(windows))]
pub(crate) fn maximize_hovered(_: &Context) -> bool {
    false
}
#[cfg(not(windows))]
pub(crate) fn publish_titlebar(_: &Context, _: &[Rect], _: Rect) {}

#[cfg(windows)]
mod platform {
    use super::*;
    use eframe::egui::{Id, Order, Pos2};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    fn key() -> Id {
        Id::new("musicfs-root-window-titlebar")
    }

    pub(in crate::system) fn register(ctx: &Context, hovered: Arc<AtomicBool>) {
        ctx.data_mut(|d| d.insert_temp(key(), hovered));
    }

    pub(crate) fn has_custom_titlebar(ctx: &Context) -> bool {
        ctx.data(|d| d.get_temp::<Arc<AtomicBool>>(key()).is_some())
    }

    pub(crate) fn maximize_hovered(ctx: &Context) -> bool {
        ctx.data(|d| {
            d.get_temp::<Arc<AtomicBool>>(key())
                .is_some_and(|value| value.load(Ordering::Relaxed))
        })
    }

    pub(crate) fn publish_titlebar(ctx: &Context, drag: &[Rect], maximize: Rect) {
        let regions = Regions {
            scale: ctx.pixels_per_point(),
            drag: drag.to_vec(),
            maximize: Some(maximize),
            overlays: Vec::new(),
        };
        ctx.data_mut(|d| d.insert_temp(key(), regions));
    }

    #[derive(Clone, Default)]
    pub(in crate::system) struct Regions {
        scale: f32,
        drag: Vec<Rect>,
        maximize: Option<Rect>,
        overlays: Vec<Rect>,
    }

    impl Regions {
        pub(in crate::system) fn take(ctx: &Context) -> Self {
            let Some(mut regions) = ctx.data_mut(|d| d.remove_temp::<Self>(key())) else {
                return Self::default();
            };
            ctx.memory(|m| {
                if m.any_popup_open() || m.top_modal_layer().is_some() {
                    return Self::default();
                }
                // 普通可移动面板也可能盖住标题栏，鼠标应交给浮层处理。
                regions.overlays.extend(
                    m.layer_ids()
                        .filter(|id| {
                            matches!(id.order, Order::Middle | Order::Foreground)
                                && m.areas().is_visible(id)
                        })
                        .filter_map(|id| m.area_rect(id.id)),
                );
                regions
            })
        }

        pub(in crate::system) fn hit(&self, x: i32, y: i32) -> u32 {
            use windows_sys::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT, HTMAXBUTTON};
            if self.scale == 0.0 {
                return HTCLIENT;
            }
            let p = Pos2::new(x as f32 / self.scale, y as f32 / self.scale);
            if self.overlays.iter().any(|r| r.contains(p)) {
                HTCLIENT
            } else if self.maximize.is_some_and(|r| r.contains(p)) {
                HTMAXBUTTON
            } else if self.drag.iter().any(|r| r.contains(p)) {
                HTCAPTION
            } else {
                HTCLIENT
            }
        }
    }
}
