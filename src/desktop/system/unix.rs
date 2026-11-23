use super::{SystemAction, SystemSender, SystemState};
use crate::i18n::tr;
use musicfs_player_core::Status;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod mpris;
#[cfg(target_os = "linux")]
mod notifier;
#[cfg(target_os = "linux")]
pub(super) use linux::UnixDesktop;

#[derive(Clone, PartialEq)]
struct Menu {
    locale: &'static str,
    track: bool,
    playing: bool,
    previous: bool,
    next: bool,
}

impl From<&SystemState> for Menu {
    fn from(state: &SystemState) -> Self {
        Self {
            locale: state.locale,
            track: state.track.is_some(),
            playing: state.status == Status::Playing,
            previous: state.can_previous,
            next: state.can_next,
        }
    }
}

impl Menu {
    fn entries(&self) -> [(&str, SystemAction, bool); 6] {
        [
            (tr("显示主窗口"), SystemAction::Show, true),
            (tr("上一首"), SystemAction::Previous, self.previous),
            (
                tr(if self.playing { "暂停" } else { "播放" }),
                SystemAction::Toggle,
                self.track,
            ),
            (tr("下一首"), SystemAction::Next, self.next),
            (tr("停止"), SystemAction::Stop, self.track),
            (tr("退出 MusicFS"), SystemAction::Quit, true),
        ]
    }
}
