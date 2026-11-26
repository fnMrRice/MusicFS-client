use musicfs_player_core::{Player, PlayerCommand, PlayerSnapshot, Status};
use std::sync::Arc;

#[cfg(test)]
#[path = "../../../tests/native/system_state.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SystemAction {
    Show,
    Quit,
    Play,
    Pause,
    Toggle,
    Previous,
    Next,
    Stop,
    Seek(u64),
    // Windows 的系统定位事件提供绝对位置。
    #[cfg_attr(windows, allow(dead_code))]
    SeekBy(i64),
    #[cfg(target_os = "linux")]
    SetVolume(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MediaTrack {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub artwork_png: Option<Arc<[u8]>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SystemState {
    pub track: Option<MediaTrack>,
    pub status: Status,
    pub can_next: bool,
    pub can_previous: bool,
    pub seek_range: Option<(u64, u64)>,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub locale: &'static str,
    #[cfg(target_os = "linux")]
    pub volume_percent: u8,
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            track: None,
            status: Status::Idle,
            can_next: false,
            can_previous: false,
            seek_range: None,
            position_ms: 0,
            duration_ms: None,
            locale: crate::i18n::locale(),
            #[cfg(target_os = "linux")]
            volume_percent: 100,
        }
    }
}

impl SystemState {
    pub fn from_snapshot(
        instance: u64,
        snapshot: &PlayerSnapshot,
        artwork_png: Option<Arc<[u8]>>,
    ) -> Self {
        let Some((index, track)) = snapshot
            .index
            .and_then(|i| snapshot.queue.get(i).map(|t| (i, t)))
        else {
            return Self {
                #[cfg(target_os = "linux")]
                volume_percent: (snapshot.volume * 100.).round() as u8,
                ..Self::default()
            };
        };
        Self {
            track: Some(MediaTrack {
                // 队列中的重复条目和不同登录会话均有独立的系统曲目标识。
                id: format!("{instance}/{index}/{}", track.id),
                title: track.title.clone(),
                artists: track.artists.clone(),
                album: track.album.clone(),
                artwork_png,
            }),
            status: snapshot.status,
            can_next: snapshot.can_next,
            can_previous: snapshot.can_previous,
            seek_range: snapshot
                .seek_range_ms
                .filter(|(start, end)| snapshot.seekable && start <= end),
            position_ms: snapshot.position_ms,
            duration_ms: snapshot.duration_ms.or(track.duration_ms),
            locale: crate::i18n::locale(),
            #[cfg(target_os = "linux")]
            volume_percent: (snapshot.volume * 100.).round() as u8,
        }
    }

    pub fn track_id(&self) -> Option<&str> {
        self.track.as_ref().map(|t| t.id.as_str())
    }

    pub fn dispatch(&self, player: &Player, action: SystemAction) -> Result<bool, String> {
        let snapshot = player.snapshot()?;
        let instance = player.presentation()?.instance;
        let now = Self::from_snapshot(instance, &snapshot, None);
        // 连续定位使用实际输出进度，换曲后丢弃上一首的定位操作。
        if matches!(action, SystemAction::Seek(_) | SystemAction::SeekBy(_))
            && now.track_id() != self.track_id()
        {
            return Ok(false);
        }
        if let Some(cmd) = now.command(action) {
            player.command(cmd)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn command(&self, action: SystemAction) -> Option<PlayerCommand> {
        self.track.as_ref()?;
        let playing = matches!(self.status, Status::Playing | Status::Loading);
        let seek = |pos: u64| {
            self.seek_range.map(|(min, max)| PlayerCommand::Seek {
                position_ms: pos.clamp(min, max),
            })
        };
        match action {
            SystemAction::Play if !playing => Some(PlayerCommand::Play),
            SystemAction::Pause if playing => Some(PlayerCommand::Pause),
            SystemAction::Toggle => Some(PlayerCommand::Toggle),
            SystemAction::Stop => Some(PlayerCommand::Stop),
            SystemAction::Next if self.can_next => Some(PlayerCommand::Next),
            SystemAction::Previous if self.can_previous => Some(PlayerCommand::Previous),
            SystemAction::Seek(pos) => seek(pos),
            SystemAction::SeekBy(delta) => seek(self.position_ms.saturating_add_signed(delta)),
            _ => None,
        }
    }
}
