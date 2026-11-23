use super::Glyphs;
use crate::{
    i18n::tr,
    system::{SystemAction, SystemState},
};
use musicfs_player_core::Status;
use windows::Win32::UI::{
    Shell::{
        THBF_DISABLED, THBF_ENABLED, THBN_CLICKED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON,
    },
    WindowsAndMessaging::HICON,
};

const PREVIOUS: u32 = 0x4d51;
const TRANSPORT: u32 = 0x4d52;
const NEXT: u32 = 0x4d53;
pub(super) const IDS: [u32; 3] = [PREVIOUS, TRANSPORT, NEXT];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Buttons {
    track: bool,
    pause: bool,
    previous: bool,
    next: bool,
    locale: &'static str,
}

struct Button {
    id: u32,
    icon: &'static str,
    label: &'static str,
    enabled: bool,
    action: SystemAction,
}

impl Buttons {
    pub fn from_state(s: &SystemState) -> Self {
        Self {
            track: s.track.is_some(),
            pause: matches!(s.status, Status::Playing | Status::Loading),
            previous: s.can_previous,
            next: s.can_next,
            locale: s.locale,
        }
    }
    fn entries(self) -> [Button; 3] {
        [
            Button {
                id: PREVIOUS,
                icon: "previous",
                label: "上一首",
                enabled: self.track && self.previous,
                action: SystemAction::Previous,
            },
            Button {
                id: TRANSPORT,
                icon: if self.pause { "pause" } else { "play" },
                label: if self.pause { "暂停" } else { "播放" },
                enabled: self.track,
                action: if self.pause {
                    SystemAction::Pause
                } else {
                    SystemAction::Play
                },
            },
            Button {
                id: NEXT,
                icon: "next",
                label: "下一首",
                enabled: self.track && self.next,
                action: SystemAction::Next,
            },
        ]
    }
    // 普通菜单/控件也会发 WM_COMMAND；只消费任务栏发送的这三个固定 ID。
    pub fn click(self, w: usize) -> Option<Option<SystemAction>> {
        if (w >> 16) & 0xffff != THBN_CLICKED as usize {
            return None;
        }
        self.entries()
            .into_iter()
            .find(|b| b.id as usize == w & 0xffff)
            .map(|b| b.enabled.then_some(b.action))
    }
    pub fn native(self, icons: &Glyphs) -> [THUMBBUTTON; 3] {
        self.entries().map(|b| {
            let mut native = THUMBBUTTON {
                dwMask: THB_FLAGS | THB_ICON | THB_TOOLTIP,
                iId: b.id,
                hIcon: HICON(icons.get(b.icon)),
                dwFlags: if b.enabled {
                    THBF_ENABLED
                } else {
                    THBF_DISABLED
                },
                ..Default::default()
            };
            for (slot, unit) in native
                .szTip
                .iter_mut()
                .take(259)
                .zip(tr(b.label).encode_utf16())
            {
                *slot = unit;
            }
            native
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn click(id: u32) -> usize {
        ((THBN_CLICKED as usize) << 16) | id as usize
    }
    #[test]
    fn thumbnail_ignores_other_commands_and_rejects_disabled_buttons() {
        let mut buttons = Buttons::from_state(&SystemState::default());
        assert_eq!(buttons.click(PREVIOUS as usize), None);
        assert_eq!(buttons.click(click(123)), None);
        for id in [PREVIOUS, TRANSPORT, NEXT] {
            assert_eq!(buttons.click(click(id)), Some(None));
        }
        buttons.track = true;
        buttons.next = true;
        assert_eq!(buttons.click(click(PREVIOUS)), Some(None));
        assert_eq!(buttons.click(click(NEXT)), Some(Some(SystemAction::Next)));
        assert_eq!(
            buttons.click(click(TRANSPORT)),
            Some(Some(SystemAction::Play))
        );
        buttons.pause = true;
        buttons.previous = true;
        assert_eq!(
            buttons.click(click(TRANSPORT)),
            Some(Some(SystemAction::Pause))
        );
        assert_eq!(
            buttons.click(click(PREVIOUS)),
            Some(Some(SystemAction::Previous))
        );
    }

    #[test]
    fn thumbnail_presentation_tracks_transport_state_but_not_playback_position() {
        use crate::system::MediaTrack;
        let mut state = SystemState::default();
        state.track = Some(MediaTrack {
            id: "track".into(),
            title: "Title".into(),
            artists: Vec::new(),
            album: None,
            artwork_png: None,
        });
        for (status, icon, label) in [
            (Status::Loading, "pause", "暂停"),
            (Status::Playing, "pause", "暂停"),
            (Status::Paused, "play", "播放"),
            (Status::Ended, "play", "播放"),
            (Status::Error, "play", "播放"),
            (Status::Idle, "play", "播放"),
        ] {
            state.status = status;
            let buttons = Buttons::from_state(&state);
            let entry = &buttons.entries()[1];
            assert_eq!((entry.icon, entry.label), (icon, label));
            state.position_ms += 500;
            assert!(buttons == Buttons::from_state(&state));
        }
        let old = Buttons::from_state(&state);
        state.locale = if state.locale == "ja" { "en" } else { "ja" };
        assert!(old != Buttons::from_state(&state));
    }
}
