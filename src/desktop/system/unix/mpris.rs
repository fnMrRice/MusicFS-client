use super::{Status, SystemAction, SystemSender, SystemState};
use mpris_server::{zbus::Result, Metadata, PlaybackStatus, Player, Time, TrackId};
use std::time::{Duration, Instant};
use std::{io::Write, sync::Arc};
use tokio::sync::watch;

// 系统媒体界面只能读取专用的本机副本。写入完成后才发布 URL，退出或换图时自动删除。
#[derive(Default)]
struct Artwork {
    bytes: Option<Arc<[u8]>>,
    file: Option<tempfile::NamedTempFile>,
    url: Option<url::Url>,
}

impl Artwork {
    fn update(&mut self, bytes: Option<&Arc<[u8]>>) -> Option<&str> {
        let changed = match (&self.bytes, bytes) {
            (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.bytes = bytes.cloned();
            self.file = bytes.and_then(|data| {
                let mut file = tempfile::Builder::new()
                    .prefix("musicfs-art-")
                    .suffix(".png")
                    .tempfile()
                    .ok()?;
                file.write_all(data).ok()?;
                Some(file)
            });
            self.url = self
                .file
                .as_ref()
                .and_then(|f| url::Url::from_file_path(f.path()).ok());
        }
        self.url.as_ref().map(url::Url::as_str)
    }
}

fn time(ms: u64) -> Time {
    Time::from_micros(ms.saturating_mul(1000).min(i64::MAX as u64) as i64)
}

fn connect(player: &Player, sender: SystemSender) {
    let callback = |action| {
        let sender = sender.clone();
        move |_: &Player| sender.send(action)
    };
    player.connect_raise(callback(SystemAction::Show));
    player.connect_quit(callback(SystemAction::Quit));
    player.connect_play(callback(SystemAction::Play));
    player.connect_pause(callback(SystemAction::Pause));
    player.connect_play_pause(callback(SystemAction::Toggle));
    player.connect_stop(callback(SystemAction::Stop));
    player.connect_next(callback(SystemAction::Next));
    player.connect_previous(callback(SystemAction::Previous));
    let volume = sender.clone();
    player.connect_set_volume(move |_, value| {
        if value.is_finite() {
            volume.send(SystemAction::SetVolume(
                (value.clamp(0.0, 1.0) * 100.0).round() as u8,
            ));
        }
    });
    let relative = sender.clone();
    player.connect_seek(move |p, offset| {
        if p.can_seek() {
            relative.send(SystemAction::SeekBy(offset.as_millis()));
        }
    });
    player.connect_set_position(move |p, id, pos| {
        let metadata = p.metadata();
        if p.can_seek()
            && metadata.trackid().as_ref() == Some(id)
            && pos.as_micros() >= 0
            && metadata.length().is_none_or(|length| pos <= length)
        {
            sender.send(SystemAction::Seek(pos.as_millis() as u64));
        }
    });
}

#[derive(Default)]
struct Published {
    artwork: Artwork,
    path: Option<TrackId>,
    last: Option<(SystemState, Instant)>,
}

impl Published {
    async fn update(&mut self, player: &Player, state: SystemState) -> Result<()> {
        let old = self.last.as_ref().map(|(s, _)| s);
        let changed = old.and_then(SystemState::track_id) != state.track_id();
        if changed {
            // 不向桌面总线暴露服务端内部曲目 ID。一个队列条目的路径在播放期间保持稳定。
            self.path = state.track.as_ref().map(|_| {
                TrackId::try_from(format!(
                    "/io/github/musicfs/track/t{}",
                    uuid::Uuid::new_v4().simple()
                ))
                .expect("Invalid generated MPRIS track path")
            });
        }
        if old.is_none_or(|s| s.track != state.track || s.duration_ms != state.duration_ms) {
            let mut meta = Metadata::new();
            let art = self
                .artwork
                .update(state.track.as_ref().and_then(|t| t.artwork_png.as_ref()));
            if let Some(track) = &state.track {
                meta.set_trackid(self.path.clone());
                meta.set_title(Some(track.title.clone()));
                meta.set_artist(Some(track.artists.clone()));
                meta.set_album(track.album.clone());
                meta.set_length(state.duration_ms.map(time));
                meta.set_art_url(art.map(str::to_owned));
            }
            player.set_metadata(meta).await?;
        }
        let status = match state.status {
            Status::Playing => PlaybackStatus::Playing,
            Status::Paused | Status::Loading => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        };
        player.set_can_go_next(state.can_next).await?;
        player.set_can_go_previous(state.can_previous).await?;
        player.set_can_play(state.track.is_some()).await?;
        player
            .set_can_pause(matches!(state.status, Status::Playing | Status::Paused))
            .await?;
        player.set_can_seek(state.seek_range.is_some()).await?;
        player.set_playback_status(status).await?;
        player
            .set_volume(state.volume_percent as f64 / 100.0)
            .await?;
        let now = Instant::now();
        let discontinuity = !changed
            && self.last.as_ref().is_some_and(|(last, observed)| {
                let expected = if last.status == Status::Playing {
                    last.position_ms
                        .saturating_add(now.duration_since(*observed).as_millis() as u64)
                } else {
                    last.position_ms
                };
                expected.abs_diff(state.position_ms) > 250
            });
        player.set_position(time(state.position_ms));
        if discontinuity {
            player.seeked(time(state.position_ms)).await?;
        }
        self.last = Some((state, now));
        Ok(())
    }
}

async fn serve(
    sender: SystemSender,
    states: &mut watch::Receiver<Option<SystemState>>,
) -> Result<()> {
    let name = format!("MusicFS.instance{}", std::process::id());
    let player = Player::builder(&name)
        .identity("MusicFS")
        .can_raise(true)
        .can_quit(true)
        .build()
        .await?;
    connect(&player, sender);
    let mut published = Published::default();
    let updates = async {
        loop {
            let state = states.borrow_and_update().clone();
            if let Some(state) = state {
                published.update(&player, state).await?;
            }
            if states.changed().await.is_err() {
                return Ok(());
            }
        }
    };
    tokio::select! { result = updates => result, _ = player.run() => Ok(()) }
}

pub(super) async fn run(sender: SystemSender, mut states: watch::Receiver<Option<SystemState>>) {
    while states.has_changed().is_ok() {
        let _ = serve(sender.clone(), &mut states).await;
        if states.has_changed().is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
