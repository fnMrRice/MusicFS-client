mod buttons;
#[cfg(test)]
mod hook_tests;
mod taskbar;

use super::{thumbnail_icons::Glyphs, wide, SystemSender, SystemState};
use buttons::Buttons;
use std::{
    cell::{Cell, RefCell},
    ptr::null_mut,
    rc::Rc,
};
use taskbar::{Apartment, Resources};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

pub(in crate::system) struct Thumbnail(Rc<Window>);

struct Window {
    hwnd: Cell<HWND>,
    created: u32,
    update: u32,
    sender: SystemSender,
    buttons: Cell<Buttons>,
    generation: Cell<u64>,
    appearance: Cell<u64>,
    posted: Cell<bool>,
    running: Cell<bool>,
    again: Cell<bool>,
    attempts: Cell<u32>,
    resources: RefCell<Resources>,
    // 确保所有 Shell COM 接口先于线程公寓释放，所有调用均在主窗口线程执行。
    _apartment: Apartment,
}

fn message(name: &str) -> Result<u32, String> {
    let id = unsafe { RegisterWindowMessageW(wide(name).as_ptr()) };
    if id == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(id)
    }
}

impl Thumbnail {
    pub(in crate::system) fn new(main: isize, sender: SystemSender) -> Result<Self, String> {
        let hwnd = main as HWND;
        let window = Rc::new(Window {
            hwnd: Cell::new(hwnd),
            created: message("TaskbarButtonCreated")?,
            update: message("MusicFS.Thumbnail.Update.1")?,
            sender,
            buttons: Cell::new(Buttons::from_state(&SystemState::default())),
            generation: Cell::new(1),
            appearance: Cell::new(1),
            posted: Cell::new(false),
            running: Cell::new(false),
            again: Cell::new(false),
            attempts: Cell::new(0),
            resources: Default::default(),
            _apartment: Apartment::enter().map_err(|e| e.to_string())?,
        });
        // eframe 在 AppCreator 返回且画出首帧后才显示窗口；先订阅主 HWND 的外壳消息。
        let owned = Rc::into_raw(window.clone());
        unsafe {
            if SetWindowSubclass(hwnd, Some(subclass), window.timer_id(), owned as usize) == 0 {
                drop(Rc::from_raw(owned));
                return Err("Could not attach the MusicFS taskbar thumbnail controls".into());
            }
            // 支持宿主窗口已显示、其首次 TaskbarButtonCreated 已交付的情况。
            window.timer(900);
        }
        Ok(Self(window))
    }
    pub(in crate::system) fn update(&self, state: &SystemState) {
        let value = Buttons::from_state(state);
        if self.0.buttons.replace(value) != value {
            self.0.request();
        }
    }
}
impl Drop for Thumbnail {
    fn drop(&mut self) {
        let window = &self.0;
        let hwnd = window.hwnd.replace(null_mut());
        if !hwnd.is_null() {
            unsafe {
                KillTimer(hwnd, window.timer_id());
                if RemoveWindowSubclass(hwnd, Some(subclass), window.timer_id()) != 0 {
                    // 与安装时交给窗口的 Rc::into_raw 配对。
                    Rc::decrement_strong_count(Rc::as_ptr(window));
                }
            }
            window
                .resources
                .take()
                .hide(windows::Win32::Foundation::HWND(hwnd));
        }
    }
}

impl Window {
    fn timer_id(&self) -> usize {
        self as *const Self as usize
    }
    unsafe fn timer(&self, ms: u32) {
        SetTimer(self.hwnd.get(), self.timer_id(), ms, None);
    }
    fn request(&self) {
        let hwnd = self.hwnd.get();
        if !hwnd.is_null() && !self.posted.replace(true) {
            unsafe {
                if PostMessageW(hwnd, self.update, 0, 0) == 0 {
                    self.posted.set(false);
                    self.timer(300);
                }
            }
        }
    }
    fn apply(&self) {
        self.posted.set(false);
        let hwnd = self.hwnd.get();
        if hwnd.is_null() || unsafe { IsWindowVisible(hwnd) } == 0 {
            return;
        }
        if self.running.replace(true) {
            self.again.set(true);
            return;
        }
        // COM 可能泵入同线程消息。调用期间不持有 RefCell 借用，重入时合并为下一次更新。
        let mut resources = self.resources.take();
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let result = resources.apply(
            windows::Win32::Foundation::HWND(hwnd),
            self.buttons.get(),
            self.generation.get(),
            self.appearance.get(),
            if dpi == 0 { 96 } else { dpi },
        );
        if !self.hwnd.get().is_null() {
            unsafe {
                KillTimer(hwnd, self.timer_id());
            }
            match result {
                Ok(()) => self.attempts.set(0),
                Err(e) => {
                    resources.disconnect();
                    let attempt = self.attempts.get();
                    if attempt == 0 {
                        eprintln!("MusicFS taskbar thumbnail controls unavailable: {e}");
                    }
                    if attempt < 6 {
                        self.attempts.set(attempt + 1);
                        unsafe {
                            self.timer(300 << attempt);
                        }
                    }
                }
            }
            self.resources.replace(resources);
        }
        self.running.set(false);
        if self.again.replace(false) {
            self.request();
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
    // 即使嵌套消息销毁窗口或 eframe 释放应用，本次回调仍独立持有状态。
    Rc::increment_strong_count(ptr);
    let window = Rc::from_raw(ptr);
    if msg == WM_NCDESTROY {
        window.hwnd.set(null_mut());
        KillTimer(hwnd, id);
        RemoveWindowSubclass(hwnd, Some(subclass), id);
        Rc::decrement_strong_count(ptr);
    } else if !window.hwnd.get().is_null() {
        if msg == window.created {
            window
                .generation
                .set(window.generation.get().wrapping_add(1));
            window.attempts.set(0);
            window.request();
        } else if msg == window.update {
            if window.posted.get() {
                window.apply();
            }
            return 0;
        } else if msg == WM_TIMER && w == id {
            KillTimer(hwnd, id);
            window.request();
            return 0;
        } else if msg == WM_COMMAND {
            if let Some(action) = window.buttons.get().click(w) {
                if let Some(action) = action {
                    window.sender.send(action);
                }
                return 0;
            }
        } else if matches!(
            msg,
            WM_DPICHANGED | WM_THEMECHANGED | WM_SYSCOLORCHANGE | WM_SETTINGCHANGE
        ) {
            window
                .appearance
                .set(window.appearance.get().wrapping_add(1));
            window.request();
        } else if msg == WM_SHOWWINDOW && w != 0 {
            window.attempts.set(0);
            window.timer(900);
        }
    }
    DefSubclassProc(hwnd, msg, w, l)
}
