use super::{Status, SystemAction, SystemSender, SystemState};
use block2::RcBlock;
use objc2::{rc::Retained, runtime::AnyObject, AnyThread};
use objc2_app_kit::NSImage;
use objc2_foundation::{NSData, NSMutableDictionary, NSNumber, NSString};
use objc2_media_player::*;
use std::{ptr::NonNull, sync::Arc};

struct Command {
    control: Retained<MPRemoteCommand>,
    target: Retained<AnyObject>,
    action: SystemAction,
}

impl Drop for Command {
    fn drop(&mut self) {
        // 仅注销本实例安装的回调，避免误删其他进程内组件的监听。
        unsafe {
            self.control.removeTarget(Some(&self.target));
        }
    }
}

pub(super) struct NowPlaying {
    center: Retained<MPNowPlayingInfoCenter>,
    commands: Vec<Command>,
    bytes: Option<Arc<[u8]>>,
    art: Option<Retained<MPMediaItemArtwork>>,
    last: Option<SystemState>,
}

impl NowPlaying {
    pub(super) fn new(sender: SystemSender) -> Self {
        // 由 eframe 主线程创建。框架保存回调，回调仅向 Rust 主循环投递命令。
        unsafe {
            let center = MPRemoteCommandCenter::sharedCommandCenter();
            let mut commands = Vec::new();
            for (control, action) in [
                (center.playCommand(), SystemAction::Play),
                (center.pauseCommand(), SystemAction::Pause),
                (center.togglePlayPauseCommand(), SystemAction::Toggle),
                (center.stopCommand(), SystemAction::Stop),
                (center.previousTrackCommand(), SystemAction::Previous),
                (center.nextTrackCommand(), SystemAction::Next),
                (
                    center.changePlaybackPositionCommand().into_super(),
                    SystemAction::Seek(0),
                ),
            ] {
                control.setEnabled(false);
                let channel = sender.clone();
                let callback = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
                    let action = if matches!(action, SystemAction::Seek(_)) {
                        let event = event.as_ref();
                        let Some(seek) =
                            event.downcast_ref::<MPChangePlaybackPositionCommandEvent>()
                        else {
                            return MPRemoteCommandHandlerStatus::CommandFailed;
                        };
                        let seconds = seek.positionTime();
                        if !seconds.is_finite() || seconds < 0.0 {
                            return MPRemoteCommandHandlerStatus::CommandFailed;
                        }
                        SystemAction::Seek((seconds * 1000.0).round() as u64)
                    } else {
                        action
                    };
                    channel.send(action);
                    MPRemoteCommandHandlerStatus::Success
                });
                let target = control.addTargetWithHandler(&callback);
                commands.push(Command {
                    control,
                    target,
                    action,
                });
            }
            Self {
                center: MPNowPlayingInfoCenter::defaultCenter(),
                commands,
                bytes: None,
                art: None,
                last: None,
            }
        }
    }

    pub(super) fn update(&mut self, state: &SystemState) {
        let previous = self.last.as_ref();
        let changed = previous.is_none_or(|s| {
            s.track != state.track || s.duration_ms != state.duration_ms || s.status != state.status
        });
        let elapsed = previous.map_or(0, |s| state.position_ms.abs_diff(s.position_ms));
        for item in &self.commands {
            let enabled = match item.action {
                SystemAction::Next => state.can_next,
                SystemAction::Previous => state.can_previous,
                SystemAction::Seek(_) => state.seek_range.is_some(),
                _ => state.track.is_some(),
            };
            unsafe {
                if item.control.isEnabled() != enabled {
                    item.control.setEnabled(enabled);
                }
            }
        }
        if !changed && elapsed < 500 && state.status == Status::Playing {
            return;
        }
        let art_bytes = state.track.as_ref().and_then(|t| t.artwork_png.as_ref());
        let same = match (&self.bytes, art_bytes) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.bytes = art_bytes.cloned();
            self.art = art_bytes.and_then(|b| artwork(b));
        }
        // 系统独立维护播放时钟，定位/暂停时用 PlayerCore 的实际进度重新校准。
        unsafe {
            if let Some(track) = &state.track {
                let info = NSMutableDictionary::<NSString, AnyObject>::new();
                info.insert(MPMediaItemPropertyTitle, &NSString::from_str(&track.title));
                info.insert(
                    MPMediaItemPropertyArtist,
                    &NSString::from_str(&track.artists.join(", ")),
                );
                if let Some(album) = &track.album {
                    info.insert(MPMediaItemPropertyAlbumTitle, &NSString::from_str(album));
                }
                if let Some(ms) = state.duration_ms {
                    info.insert(
                        MPMediaItemPropertyPlaybackDuration,
                        &NSNumber::new_f64(ms as f64 / 1000.0),
                    );
                }
                if let Some(art) = &self.art {
                    info.insert(MPMediaItemPropertyArtwork, art);
                }
                info.insert(
                    MPNowPlayingInfoPropertyElapsedPlaybackTime,
                    &NSNumber::new_f64(state.position_ms as f64 / 1000.0),
                );
                info.insert(
                    MPNowPlayingInfoPropertyPlaybackRate,
                    &NSNumber::new_f64(if state.status == Status::Playing {
                        1.0
                    } else {
                        0.0
                    }),
                );
                self.center.setNowPlayingInfo(Some(&info));
            } else {
                self.center.setNowPlayingInfo(None);
            }
            self.center.setPlaybackState(match state.status {
                Status::Playing => MPNowPlayingPlaybackState::Playing,
                Status::Paused | Status::Loading => MPNowPlayingPlaybackState::Paused,
                _ => MPNowPlayingPlaybackState::Stopped,
            });
        }
        self.last = Some(state.clone());
    }
}

fn artwork(bytes: &[u8]) -> Option<Retained<MPMediaItemArtwork>> {
    let data = NSData::with_bytes(bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    let size = image.size();
    // 封面是只读的。系统复制该块并持有其中的图像，在请求期间返回有效对象。
    let callback = RcBlock::new(move |_| NonNull::from(&*image));
    Some(unsafe {
        MPMediaItemArtwork::initWithBoundsSize_requestHandler(
            MPMediaItemArtwork::alloc(),
            size,
            &callback,
        )
    })
}

impl Drop for NowPlaying {
    fn drop(&mut self) {
        unsafe {
            self.center.setNowPlayingInfo(None);
            self.center
                .setPlaybackState(MPNowPlayingPlaybackState::Stopped);
        }
    }
}
