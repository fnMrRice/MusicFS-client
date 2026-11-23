use super::{MediaTrack, SystemAction, SystemSender, SystemState};
use musicfs_player_core::Status;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::{factory, Error, Result as WinResult, HSTRING};
use windows::Foundation::{EventRegistrationToken, TimeSpan, TypedEventHandler};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    SystemMediaTransportControls as Controls, SystemMediaTransportControlsButton as Button,
    SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::{IRandomAccessStream, RandomAccessStreamReference};
use windows::Win32::Foundation::{E_OUTOFMEMORY, HWND, RPC_E_CHANGED_MODE};
use windows::Win32::System::WinRT::{
    CreateRandomAccessStreamOverStream, ISystemMediaTransportControlsInterop, RoInitialize,
    RoUninitialize, BSOS_DEFAULT, RO_INIT_SINGLETHREADED,
};
use windows::Win32::UI::Shell::SHCreateMemStream;

// 只在创建主 HWND 的线程初始化、释放该线程的 WinRT 引用。
struct Apartment(bool, PhantomData<Rc<()>>);

impl Apartment {
    fn enter() -> WinResult<Self> {
        let owned = match unsafe { RoInitialize(RO_INIT_SINGLETHREADED) } {
            Ok(()) => true,
            Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
            Err(error) => return Err(error),
        };
        Ok(Self(owned, PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { RoUninitialize() }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Commands {
    play: bool,
    pause: bool,
    next: bool,
    previous: bool,
    seek: Option<(u64, u64)>,
}

impl Commands {
    fn from_state(state: &SystemState) -> Self {
        Self {
            play: !matches!(state.status, Status::Playing | Status::Loading),
            pause: state.status == Status::Playing,
            next: state.can_next,
            previous: state.can_previous,
            seek: state.seek_range.filter(|(start, end)| start < end),
        }
    }

    fn button(self, value: Button) -> Option<SystemAction> {
        match value {
            Button::Play if self.play => Some(SystemAction::Play),
            Button::Pause if self.pause => Some(SystemAction::Pause),
            Button::Next if self.next => Some(SystemAction::Next),
            Button::Previous if self.previous => Some(SystemAction::Previous),
            Button::Stop => Some(SystemAction::Stop),
            _ => None,
        }
    }
}

pub(super) struct WindowsMedia {
    controls: Controls,
    buttons: Option<EventRegistrationToken>,
    seeks: Option<EventRegistrationToken>,
    sender: SystemSender,
    commands: Arc<Mutex<Option<Commands>>>,
    last: Option<SystemState>,
    retry_after: Option<Instant>,
    _apartment: Apartment,
}

impl WindowsMedia {
    pub(super) fn new(hwnd: isize, sender: SystemSender) -> Result<Self, String> {
        Self::attach(hwnd, sender).map_err(|error| error.to_string())
    }

    fn attach(hwnd: isize, sender: SystemSender) -> WinResult<Self> {
        let apartment = Apartment::enter()?;
        let factory = factory::<Controls, ISystemMediaTransportControlsInterop>()?;
        let controls: Controls = unsafe { factory.GetForWindow(HWND(hwnd as *mut _))? };
        controls.SetIsEnabled(false)?;
        let mut this = Self {
            controls,
            buttons: None,
            seeks: None,
            sender: sender.clone(),
            commands: Arc::new(Mutex::new(None)),
            last: None,
            retry_after: None,
            _apartment: apartment,
        };
        let commands = this.commands.clone();
        let button = TypedEventHandler::<
            Controls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_, args| {
            if let Some(args) = args {
                let value = args.Button()?;
                let action = commands.lock().unwrap().and_then(|c| c.button(value));
                if let Some(action) = action {
                    sender.send(action);
                }
            }
            Ok(())
        });
        this.buttons = Some(this.controls.ButtonPressed(&button)?);
        Ok(this)
    }

    fn enable_seeking(&mut self, enabled: bool) -> WinResult<()> {
        if self.seeks.is_some() == enabled {
            return Ok(());
        }
        if let Some(token) = self.seeks {
            self.controls.RemovePlaybackPositionChangeRequested(token)?;
            self.seeks = None;
            return Ok(());
        }
        let commands = self.commands.clone();
        let sender = self.sender.clone();
        let seek = TypedEventHandler::<Controls, PlaybackPositionChangeRequestedEventArgs>::new(
            move |_, args| {
                if let Some(args) = args {
                    let range = commands.lock().unwrap().and_then(|c| c.seek);
                    if let Some((start, end)) = range {
                        let ticks = args.RequestedPlaybackPosition()?.Duration.max(0) as u64;
                        sender.send(SystemAction::Seek((ticks / 10_000).clamp(start, end)));
                    }
                }
                Ok(())
            },
        );
        self.seeks = Some(self.controls.PlaybackPositionChangeRequested(&seek)?);
        Ok(())
    }

    pub(super) fn update(&mut self, state: &SystemState) {
        // 登出/清空立即撤销按键，即使 Windows 正在重启媒体服务。
        if state.track.is_none() {
            *self.commands.lock().unwrap() = None;
        }
        if self.retry_after.is_some_and(|t| t > Instant::now()) && state.track.is_some() {
            return;
        }
        match self.publish(state) {
            Ok(()) => self.retry_after = None,
            Err(error) => {
                if self.retry_after.is_none() {
                    eprintln!("Could not update Windows media controls: {error}");
                }
                self.retry_after = Some(Instant::now() + Duration::from_secs(1));
            }
        }
    }

    fn publish(&mut self, state: &SystemState) -> WinResult<()> {
        let Some(track) = state.track.as_ref() else {
            if self.last.is_some() || self.retry_after.is_some() {
                self.clear()?;
                self.enable_seeking(false)?;
                self.last = None;
            }
            return Ok(());
        };
        let commands = Commands::from_state(state);
        self.enable_seeking(commands.seek.is_some())?;
        let previous = self.last.as_ref();
        let old_track = previous.and_then(|s| s.track.as_ref());
        let metadata_changed = !old_track.is_some_and(|old| same_metadata(old, track));
        let commands_changed = previous.is_none_or(|s| Commands::from_state(s) != commands);
        if commands_changed {
            self.controls.SetIsPlayEnabled(commands.play)?;
            self.controls.SetIsPauseEnabled(commands.pause)?;
            self.controls.SetIsNextEnabled(commands.next)?;
            self.controls.SetIsPreviousEnabled(commands.previous)?;
            self.controls.SetIsStopEnabled(true)?;
            *self.commands.lock().unwrap() = Some(commands);
        }
        if metadata_changed {
            self.metadata(track)?;
        }
        let status_changed = previous.is_none_or(|s| s.status != state.status);
        if status_changed {
            let status = match state.status {
                Status::Playing => MediaPlaybackStatus::Playing,
                Status::Paused => MediaPlaybackStatus::Paused,
                Status::Loading => MediaPlaybackStatus::Changing,
                _ => MediaPlaybackStatus::Stopped,
            };
            self.controls.SetPlaybackStatus(status)?;
        }
        let timeline_changed = metadata_changed
            || commands_changed
            || status_changed
            || previous.is_none_or(|s| {
                s.duration_ms != state.duration_ms
                    || s.seek_range != state.seek_range
                    || position_due(s.position_ms, state.position_ms, state.status)
            });
        if timeline_changed {
            self.timeline(state)?;
        }
        if previous.is_none() {
            self.controls.SetIsEnabled(true)?;
        }
        if timeline_changed {
            self.last = Some(state.clone());
        }
        Ok(())
    }

    fn metadata(&self, track: &MediaTrack) -> WinResult<()> {
        let updater = self.controls.DisplayUpdater()?;
        updater.ClearAll()?;
        updater.SetType(MediaPlaybackType::Music)?;
        updater.SetAppMediaId(&HSTRING::from(&track.id))?;
        let music = updater.MusicProperties()?;
        music.SetTitle(&HSTRING::from(&track.title))?;
        music.SetArtist(&HSTRING::from(track.artists.join(", ")))?;
        music.SetAlbumTitle(&HSTRING::from(track.album.as_deref().unwrap_or_default()))?;
        if let Some(png) = &track.artwork_png {
            if let Err(error) = thumbnail(png).and_then(|image| updater.SetThumbnail(&image)) {
                eprintln!("Could not publish Windows media artwork: {error}");
            }
        }
        updater.Update()
    }

    fn timeline(&self, state: &SystemState) -> WinResult<()> {
        let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
        let (start, end) = state.seek_range.unwrap_or_default();
        let duration = state
            .duration_ms
            .unwrap_or_else(|| end.max(state.position_ms));
        timeline.SetStartTime(span(0))?;
        timeline.SetEndTime(span(duration))?;
        timeline.SetMinSeekTime(span(start.min(duration)))?;
        timeline.SetMaxSeekTime(span(end.min(duration)))?;
        timeline.SetPosition(span(state.position_ms.min(duration)))?;
        self.controls.UpdateTimelineProperties(&timeline)
    }

    fn clear(&self) -> WinResult<()> {
        self.controls.SetIsEnabled(false)?;
        self.controls
            .SetPlaybackStatus(MediaPlaybackStatus::Closed)?;
        let updater = self.controls.DisplayUpdater()?;
        updater.ClearAll()?;
        updater.Update()
    }
}

fn thumbnail(png: &[u8]) -> WinResult<RandomAccessStreamReference> {
    // Windows 持有流的引用，切换曲目后无需保留文件或服务端地址。
    let stream = unsafe { SHCreateMemStream(Some(png)) }.ok_or(Error::from(E_OUTOFMEMORY))?;
    let random: IRandomAccessStream =
        unsafe { CreateRandomAccessStreamOverStream(&stream, BSOS_DEFAULT)? };
    RandomAccessStreamReference::CreateFromStream(&random)
}

fn same_metadata(a: &MediaTrack, b: &MediaTrack) -> bool {
    a.id == b.id
        && a.title == b.title
        && a.artists == b.artists
        && a.album == b.album
        && match (&a.artwork_png, &b.artwork_png) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
}

fn span(ms: u64) -> TimeSpan {
    TimeSpan {
        Duration: ms.saturating_mul(10_000).min(i64::MAX as u64) as i64,
    }
}

fn position_due(published: u64, sampled: u64, status: Status) -> bool {
    sampled < published
        || sampled.saturating_sub(published) >= 1000
        || (status != Status::Playing && sampled != published)
}

impl Drop for WindowsMedia {
    fn drop(&mut self) {
        *self.commands.lock().unwrap() = None;
        if let Some(token) = self.buttons.take() {
            let _ = self.controls.RemoveButtonPressed(token);
        }
        if let Some(token) = self.seeks.take() {
            let _ = self.controls.RemovePlaybackPositionChangeRequested(token);
        }
        let _ = self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequent_samples_advance_the_timeline_and_paused_seeks_publish_immediately() {
        let mut published = 0;
        let mut updates = Vec::new();
        for sample in (160..=2560).step_by(160) {
            if position_due(published, sample, Status::Playing) {
                updates.push(sample);
                published = sample;
            }
        }
        assert_eq!(updates, [1120, 2240]);
        assert!(position_due(published, published + 1, Status::Paused));
        assert!(position_due(published, published - 1, Status::Playing));
        assert!(!position_due(published, published, Status::Paused));
    }
}
