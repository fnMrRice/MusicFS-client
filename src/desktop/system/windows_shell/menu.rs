use super::{wide, SystemAction, SystemState, Window};
use crate::i18n::tr;
use musicfs_player_core::Status;
use std::ptr::null;
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::*,
};

#[derive(PartialEq, Eq)]
pub(super) struct Presentation {
    pub tip: String,
    pub playing: bool,
    pub track: bool,
    pub previous: bool,
    pub next: bool,
}
impl Default for Presentation {
    fn default() -> Self {
        Self::from_state(&SystemState::default())
    }
}
impl Presentation {
    pub fn from_state(state: &SystemState) -> Self {
        let tip = match &state.track {
            None => "MusicFS".into(),
            Some(track) if track.artists.is_empty() => format!("MusicFS · {}", track.title),
            Some(track) => format!("MusicFS · {} — {}", track.title, track.artists.join(", ")),
        };
        Self {
            tip,
            playing: state.status == Status::Playing,
            track: state.track.is_some(),
            previous: state.can_previous,
            next: state.can_next,
        }
    }
    pub fn allows(&self, action: SystemAction) -> bool {
        match action {
            SystemAction::Previous => self.previous,
            SystemAction::Next => self.next,
            SystemAction::Toggle | SystemAction::Stop => self.track,
            SystemAction::Show | SystemAction::Quit => true,
            _ => false,
        }
    }
}

pub(super) const ITEMS: [(SystemAction, &str, &str); 6] = [
    (SystemAction::Show, "show", "显示主窗口"),
    (SystemAction::Previous, "previous", "上一首"),
    (SystemAction::Toggle, "toggle", "播放 / 暂停"),
    (SystemAction::Next, "next", "下一首"),
    (SystemAction::Stop, "stop", "停止"),
    (SystemAction::Quit, "quit", "退出 MusicFS"),
];

pub(super) unsafe fn popup(hwnd: HWND, window: &Window, at: POINT) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    {
        let state = window.state.borrow();
        for (i, (action, _, label)) in ITEMS.iter().enumerate() {
            if i == 1 || *action == SystemAction::Quit {
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
            }
            let label = if *action == SystemAction::Toggle {
                if state.playing {
                    "暂停"
                } else {
                    "播放"
                }
            } else {
                label
            };
            let flags = if state.allows(*action) {
                MF_STRING
            } else {
                MF_STRING | MF_GRAYED
            };
            AppendMenuW(menu, flags, i + 1, wide(tr(label)).as_ptr());
        }
    }
    // Windows 需要前台所有者和一个后续消息，才能在点菜单外时正确关闭菜单。
    SetForegroundWindow(hwnd);
    let id = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        at.x,
        at.y,
        0,
        hwnd,
        null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if let Some((action, _, _)) = id.checked_sub(1).and_then(|i| ITEMS.get(i as usize)) {
        if window.state.borrow().allows(*action) {
            window.sender.send(*action);
        }
    }
}
