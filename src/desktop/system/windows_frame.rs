use super::titlebar::{self, Regions};
use eframe::egui::{Context, ViewportId};
use std::{
    cell::{Cell, RefCell},
    mem::size_of,
    ptr::null_mut,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    UI::{
        HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
        Input::KeyboardAndMouse::{
            GetCapture, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TME_NONCLIENT,
            TRACKMOUSEEVENT,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

pub(super) struct Frame(Rc<Window>);

struct Window {
    hwnd: Cell<HWND>,
    regions: RefCell<Regions>,
    context: Context,
    hovered: Arc<AtomicBool>,
    pressed: Cell<bool>,
}

impl Frame {
    pub(super) fn new(main: isize, ctx: &Context) -> Result<Self, String> {
        let hwnd = main as HWND;
        let window = Rc::new(Window {
            hwnd: Cell::new(hwnd),
            regions: RefCell::default(),
            context: ctx.clone(),
            hovered: Arc::default(),
            pressed: Cell::new(false),
        });
        let ptr = Rc::into_raw(window.clone());
        unsafe {
            if SetWindowSubclass(hwnd, Some(subclass), ptr as usize, ptr as usize) == 0 {
                drop(Rc::from_raw(ptr));
                return Err("Could not attach the MusicFS window frame".into());
            }
            SetWindowPos(
                hwnd,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
        titlebar::register(ctx, window.hovered.clone());
        Ok(Self(window))
    }

    pub(super) fn update(&self, ctx: &Context) {
        self.0.regions.replace(Regions::take(ctx));
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let ptr = Rc::as_ptr(&self.0);
        let hwnd = self.0.hwnd.replace(null_mut());
        if !hwnd.is_null() {
            unsafe {
                if RemoveWindowSubclass(hwnd, Some(subclass), ptr as usize) != 0 {
                    Rc::decrement_strong_count(ptr);
                }
            }
        }
    }
}

fn point(lparam: LPARAM) -> POINT {
    POINT {
        x: lparam as i16 as i32,
        y: (lparam >> 16) as i16 as i32,
    }
}

fn resize_hit(rect: RECT, p: POINT, x: i32, y: i32) -> Option<u32> {
    if p.x < rect.left || p.y < rect.top || p.x >= rect.right || p.y >= rect.bottom {
        return None;
    }
    match (
        p.x < rect.left + x,
        p.x >= rect.right - x,
        p.y < rect.top + y,
        p.y >= rect.bottom - y,
    ) {
        (true, _, true, _) => Some(HTTOPLEFT),
        (_, true, true, _) => Some(HTTOPRIGHT),
        (true, _, _, true) => Some(HTBOTTOMLEFT),
        (_, true, _, true) => Some(HTBOTTOMRIGHT),
        (true, _, _, _) => Some(HTLEFT),
        (_, true, _, _) => Some(HTRIGHT),
        (_, _, true, _) => Some(HTTOP),
        (_, _, _, true) => Some(HTBOTTOM),
        _ => None,
    }
}

impl Window {
    unsafe fn hit(&self, hwnd: HWND, mut p: POINT) -> u32 {
        let mut rect = RECT::default();
        if IsZoomed(hwnd) == 0
            && GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_THICKFRAME != 0
            && GetWindowRect(hwnd, &mut rect) != 0
        {
            let dpi = GetDpiForWindow(hwnd);
            let padding = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
            if let Some(edge) = resize_hit(
                rect,
                p,
                GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + padding,
                GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + padding,
            ) {
                return edge;
            }
        }
        if ScreenToClient(hwnd, &mut p) == 0 {
            return HTCLIENT;
        }
        self.regions.borrow().hit(p.x, p.y)
    }

    fn hover(&self, on: bool) {
        if self.hovered.swap(on, Ordering::Relaxed) != on {
            self.context.request_repaint_of(ViewportId::ROOT);
        }
    }
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    let ptr = data as *const Window;
    // 同步消息可以重入并销毁 HWND，回调必须持有自己的引用。
    Rc::increment_strong_count(ptr);
    let window = Rc::from_raw(ptr);
    match msg {
        WM_NCDESTROY => {
            window.hwnd.set(null_mut());
            RemoveWindowSubclass(hwnd, Some(subclass), id);
            Rc::decrement_strong_count(ptr);
        }
        WM_NCCALCSIZE
            if w != 0
                && IsZoomed(hwnd) == 0
                && GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_THICKFRAME != 0 =>
        {
            // 沿用 winit 的无边框阴影做法，保留一个物理像素的非客户区。
            // 最大化时直接使用 winit 根据当前显示器与任务栏算出的工作区。
            let result = DefSubclassProc(hwnd, msg, w, l);
            let rect = &mut (*(l as *mut NCCALCSIZE_PARAMS)).rgrc[0];
            rect.top += 1;
            rect.bottom += 1;
            return result;
        }
        WM_NCHITTEST => return window.hit(hwnd, point(l)) as LRESULT,
        WM_NCMOUSEMOVE => {
            let on = w == HTMAXBUTTON as usize;
            if on && !window.hovered.load(Ordering::Relaxed) {
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_NONCLIENT | TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tracking);
            }
            window.hover(on);
            // 继续分发非客户区悬停，Windows 11 才能显示贴靠布局。
        }
        WM_NCMOUSELEAVE => window.hover(false),
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if w == HTMAXBUTTON as usize => {
            window.pressed.set(true);
            SetCapture(hwnd);
            return 0;
        }
        WM_MOUSEMOVE if window.pressed.get() => {
            let p = point(l);
            window.hover(window.regions.borrow().hit(p.x, p.y) == HTMAXBUTTON);
            return 0;
        }
        WM_LBUTTONUP | WM_NCLBUTTONUP if window.pressed.replace(false) => {
            let mut p = point(l);
            if msg == WM_NCLBUTTONUP {
                ScreenToClient(hwnd, &mut p);
            }
            let activate = window.regions.borrow().hit(p.x, p.y) == HTMAXBUTTON;
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            window.hover(false);
            if activate {
                let command = if IsZoomed(hwnd) != 0 {
                    SC_RESTORE
                } else {
                    SC_MAXIMIZE
                };
                PostMessageW(hwnd, WM_SYSCOMMAND, command as usize, 0);
            }
            return 0;
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => {
            window.pressed.set(false);
            window.hover(false);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, w, l)
}

#[cfg(test)]
mod tests;
