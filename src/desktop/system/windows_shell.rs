mod menu;
mod tray;

use super::{SystemAction, SystemSender, SystemState};
use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::{LoadIconMetric, LIM_SMALL},
        WindowsAndMessaging::*,
    },
};

const CLASS: &str = "MusicFS.Shell.1";
const TRAY_EVENT: u32 = WM_APP + 17;
const RETRY: usize = 1;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

struct Window {
    sender: SystemSender,
    icon: HICON,
    taskbar_created: u32,
    registered: Cell<bool>,
    state: RefCell<menu::Presentation>,
}

pub(crate) struct WindowsShell {
    hwnd: HWND,
    window: Box<Window>,
}

impl WindowsShell {
    pub(crate) fn new(_main: isize, sender: SystemSender) -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(null());
            let class = wide(CLASS);
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut icon = null_mut();
            if LoadIconMetric(instance, 1 as *const u16, LIM_SMALL, &mut icon) < 0 {
                return Err("Could not load the notification area icon".into());
            }
            let window = Box::new(Window {
                sender,
                icon,
                taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
                registered: Cell::new(false),
                state: Default::default(),
            });
            // 不使用 HWND_MESSAGE：只有顶层窗口会收到 Explorer 重启后的广播。
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                wide("MusicFS").as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                (&*window as *const Window).cast(),
            );
            if hwnd.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            tray::register(hwnd, &window);
            Ok(Self { hwnd, window })
        }
    }

    pub(crate) fn update(&mut self, state: &SystemState) {
        let next = menu::Presentation::from_state(state);
        if *self.window.state.borrow() != next {
            let tooltip_changed = self.window.state.borrow().tip != next.tip;
            self.window.state.replace(next);
            if tooltip_changed && self.window.registered.get() {
                unsafe { tray::update_tip(self.hwnd, &self.window) };
            }
        }
    }
}

impl Drop for WindowsShell {
    fn drop(&mut self) {
        unsafe {
            KillTimer(self.hwnd, RETRY);
            tray::remove(self.hwnd);
            DestroyWindow(self.hwnd);
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe { DestroyIcon(self.icon) };
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Window;
    let Some(window) = ptr.as_ref() else {
        return DefWindowProcW(hwnd, msg, w, l);
    };
    if msg == window.taskbar_created && msg != 0 {
        window.registered.set(false);
        tray::register(hwnd, window);
        return 0;
    }
    match msg {
        WM_TIMER if w == RETRY => tray::register(hwnd, window),
        TRAY_EVENT => tray::event(hwnd, window, w, l),
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            return DefWindowProcW(hwnd, msg, w, l);
        }
        _ => return DefWindowProcW(hwnd, msg, w, l),
    }
    0
}
