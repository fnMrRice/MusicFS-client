mod activation;
mod jump_list;
mod menu;
mod tray;

pub(crate) use activation::handle_activation;

use super::{SystemAction, SystemSender, SystemState};
use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
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
    activation: u32,
    tasks: Option<jump_list::JumpList>,
    locale: Cell<&'static str>,
}

pub(crate) struct WindowsShell {
    hwnd: HWND,
    window: Box<Window>,
}

impl WindowsShell {
    pub(crate) fn new(main: isize, sender: SystemSender) -> Result<Self, String> {
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
            let icon = LoadImageW(
                instance,
                1 as *const u16,
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                0,
            );
            if icon.is_null() {
                return Err("Could not load the notification area icon".into());
            }
            let identity = activation::identity();
            let token = identity
                .as_ref()
                .map(|id| id.token)
                .unwrap_or_else(|_| uuid::Uuid::new_v4());
            let name = wide(&activation::title(token));
            let tasks = identity
                .and_then(|id| jump_list::JumpList::new(main, id).map_err(|e| e.to_string()))
                .map_err(|e| eprintln!("MusicFS taskbar initialization failed: {e}"))
                .ok();
            let window = Box::new(Window {
                sender,
                icon,
                activation: RegisterWindowMessageW(name.as_ptr()),
                tasks,
                locale: Cell::new(crate::i18n::locale()),
                taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
                registered: Cell::new(false),
                state: Default::default(),
            });
            // 不使用 HWND_MESSAGE：只有顶层窗口会收到 Explorer 重启后的广播。
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                name.as_ptr(),
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
        if self.window.locale.replace(state.locale) != state.locale {
            if let Some(tasks) = &self.window.tasks {
                tasks.refresh();
            }
        }
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
        if let Some(tasks) = &window.tasks {
            tasks.refresh();
        }
        return 0;
    }
    if msg == window.activation && msg != 0 {
        if let Some((action, _, _)) = w.checked_sub(1).and_then(|i| menu::ITEMS.get(i)) {
            if window.state.borrow().allows(*action) {
                window.sender.send(*action);
                return 1;
            }
        }
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
