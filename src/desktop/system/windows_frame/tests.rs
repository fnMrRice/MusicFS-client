use super::*;
use eframe::egui::{self, pos2, Rect};
use windows_sys::Win32::{Graphics::Gdi::ClientToScreen, System::LibraryLoader::GetModuleHandleW};

fn packed(p: POINT) -> LPARAM {
    (u32::from(p.x as u16) | (u32::from(p.y as u16) << 16)) as LPARAM
}

#[test]
fn negative_monitor_coordinates_hit_all_resize_edges() {
    let rect = RECT {
        left: -500,
        top: -200,
        right: -100,
        bottom: 100,
    };
    for (x, y, hit) in [
        (-500, -200, HTTOPLEFT),
        (-101, -199, HTTOPRIGHT),
        (-498, 99, HTBOTTOMLEFT),
        (-101, 99, HTBOTTOMRIGHT),
        (-250, -195, HTTOP),
        (-497, -20, HTLEFT),
        (-106, 25, HTRIGHT),
        (-250, 93, HTBOTTOM),
    ] {
        let p = point(packed(POINT { x, y }));
        assert_eq!(resize_hit(rect, p, 8, 8), Some(hit));
    }
    assert_eq!(resize_hit(rect, POINT { x: -250, y: -120 }, 8, 8), None);
}

struct TestWindow(HWND);

impl TestWindow {
    fn new() -> Self {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                std::ptr::null(),
                WS_OVERLAPPEDWINDOW,
                40,
                40,
                640,
                480,
                null_mut(),
                null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        Self(hwnd)
    }

    fn message_at(&self, msg: u32, hit: u32, x: i32, y: i32) -> LRESULT {
        let mut p = POINT { x, y };
        unsafe {
            assert_ne!(ClientToScreen(self.0, &mut p), 0);
            SendMessageW(self.0, msg, hit as usize, packed(p))
        }
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}

#[test]
fn custom_caption_honors_egui_scale_and_popups_on_the_native_window() {
    let root = TestWindow::new();
    let ctx = Context::default();
    ctx.set_pixels_per_point(2.);
    let frame = Frame::new(root.0 as isize, &ctx).unwrap();
    assert!(titlebar::has_custom_titlebar(&ctx));
    let repaint = |popup| {
        let _ = ctx.run(Default::default(), |ctx| {
            titlebar::publish_titlebar(
                ctx,
                &[Rect::from_min_max(pos2(10., 10.), pos2(80., 30.))],
                Rect::from_min_max(pos2(110., 10.), pos2(130., 30.)),
            );
            if popup {
                ctx.memory_mut(|m| m.open_popup(egui::Id::new("test-popup")));
            }
            frame.update(ctx);
        });
    };
    repaint(false);
    assert_eq!(
        root.message_at(WM_NCHITTEST, 0, 120, 40),
        HTCAPTION as isize
    );
    assert_eq!(root.message_at(WM_NCHITTEST, 0, 200, 40), HTCLIENT as isize);
    assert_eq!(
        root.message_at(WM_NCHITTEST, 0, 240, 40),
        HTMAXBUTTON as isize
    );
    root.message_at(WM_NCLBUTTONDOWN, HTMAXBUTTON, 240, 40);
    unsafe {
        SendMessageW(root.0, WM_LBUTTONUP, 0, packed(POINT { x: 200, y: 40 }));
    }
    let mut msg = MSG::default();
    assert_eq!(
        unsafe { PeekMessageW(&mut msg, root.0, WM_SYSCOMMAND, WM_SYSCOMMAND, PM_REMOVE) },
        0
    );
    root.message_at(WM_NCLBUTTONDOWN, HTMAXBUTTON, 240, 40);
    unsafe {
        SendMessageW(root.0, WM_LBUTTONUP, 0, packed(POINT { x: 240, y: 40 }));
    }
    assert_ne!(
        unsafe { PeekMessageW(&mut msg, root.0, WM_SYSCOMMAND, WM_SYSCOMMAND, PM_REMOVE) },
        0
    );
    assert_eq!(msg.wParam, SC_MAXIMIZE as usize);
    repaint(true);
    assert_eq!(root.message_at(WM_NCHITTEST, 0, 120, 40), HTCLIENT as isize);
    assert_eq!(root.message_at(WM_NCHITTEST, 0, 240, 40), HTCLIENT as isize);
    drop(root);
    assert!(frame.0.hwnd.get().is_null());
    assert_eq!(Rc::strong_count(&frame.0), 1);
}
