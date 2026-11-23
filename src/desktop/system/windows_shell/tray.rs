use super::{menu, SystemAction, Window, RETRY, TRAY_EVENT};
use std::mem::size_of;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
    UI::{Shell::*, WindowsAndMessaging::*},
};

const ID: u32 = 1;

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID,
        ..Default::default()
    }
}

fn fill_tip(data: &mut NOTIFYICONDATAW, text: &str) {
    // 按 Unicode 字符边界截断，保留结尾 NUL 并避免拆散 UTF-16 代理对。
    let mut tip = [0u16; 128];
    let mut length = 0;
    for c in text.chars() {
        if length + c.len_utf16() >= tip.len() || c == '\0' {
            break;
        }
        length += c.encode_utf16(&mut tip[length..]).len();
    }
    data.szTip = tip;
}

pub(super) unsafe fn register(hwnd: HWND, window: &Window) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = TRAY_EVENT;
    nid.hIcon = window.icon;
    fill_tip(&mut nid, &window.state.borrow().tip);
    if Shell_NotifyIconW(NIM_ADD, &nid) != 0 {
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if Shell_NotifyIconW(NIM_SETVERSION, &nid) != 0 {
            window.registered.set(true);
            KillTimer(hwnd, RETRY);
            return;
        }
        remove(hwnd);
    }
    SetTimer(hwnd, RETRY, 2000, None);
}

pub(super) unsafe fn update_tip(hwnd: HWND, window: &Window) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_TIP | NIF_SHOWTIP;
    fill_tip(&mut nid, &window.state.borrow().tip);
    if Shell_NotifyIconW(NIM_MODIFY, &nid) == 0 {
        window.registered.set(false);
        SetTimer(hwnd, RETRY, 2000, None);
    }
}

pub(super) unsafe fn remove(hwnd: HWND) {
    Shell_NotifyIconW(NIM_DELETE, &data(hwnd));
}

pub(super) unsafe fn event(hwnd: HWND, window: &Window, w: WPARAM, l: LPARAM) {
    if ((l as usize >> 16) & 0xffff) != ID as usize {
        return;
    }
    let event = l as u32 & 0xffff;
    if event == NIN_SELECT || event == NIN_SELECT + 1 {
        window.sender.send(SystemAction::Show);
    } else if event == WM_CONTEXTMENU {
        let mut at = POINT {
            x: w as i16 as i32,
            y: (w >> 16) as i16 as i32,
        };
        if at.x == -1 && at.y == -1 {
            let id = NOTIFYICONIDENTIFIER {
                cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: hwnd,
                uID: ID,
                ..Default::default()
            };
            let mut rect = RECT::default();
            if Shell_NotifyIconGetRect(&id, &mut rect) >= 0 {
                at = POINT {
                    x: rect.left,
                    y: rect.bottom,
                };
            } else {
                GetCursorPos(&mut at);
            }
        }
        menu::popup(hwnd, window, at);
        Shell_NotifyIconW(NIM_SETFOCUS, &data(hwnd));
    }
}
