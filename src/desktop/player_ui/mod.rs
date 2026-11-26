use crate::i18n::tr;
mod bar;
mod controls;
pub(crate) mod cover;
mod lyrics;
mod queue_list;
mod queue_window;
mod windows;

pub(crate) use controls::PLAYBACK_MODES;
use crate::ui::style;
use cover::Cover;
use eframe::egui::{self, Context};
use musicfs_player_core::{PlaybackMode, Player, PlayerCommand, PlayerSnapshot, Status, Track};
use musicfs_server_adapter::ServerSession;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use windows::Windows;

const TICK: Duration = Duration::from_millis(160);

pub(crate) enum BarChange {
    Volume(f32),
    PlaybackMode(PlaybackMode),
}

#[derive(Default)]
pub struct PlayerUi {
    snapshot: PlayerSnapshot,
    identity: Option<(u64, u64)>,
    last_poll: Option<Instant>,
    last_full: Option<Instant>,
    error: Option<String>,
    seek_draft: Option<u64>,
    queue: queue_window::QueueView,
    cover: Cover,
    windows: Windows,
}

impl PlayerUi {
    pub fn system_state(&self) -> crate::system::SystemState {
        let Some((instance, _)) = self.identity else {
            return Default::default();
        };
        let cover = self.current_track().and_then(Track::cover);
        crate::system::SystemState::from_snapshot(
            instance,
            &self.snapshot,
            self.cover.png_for(instance, cover),
        )
    }

    pub fn status(&self) -> Status {
        self.snapshot.status
    }

    pub fn refresh(&mut self) {
        self.last_full = None;
        self.last_poll = None;
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.snapshot
            .index
            .and_then(|index| self.snapshot.queue.get(index))
    }

    pub fn current_source_id(&self) -> Option<&str> {
        self.snapshot.active_source_track_id.as_deref().or_else(|| {
            self.current_track()
                .and_then(|track| track.source_track_id.as_deref())
        })
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        player: &Player,
        server: &ServerSession,
        runtime: &Handle,
        volume: f32,
        last_volume: f32,
    ) -> Option<BarChange> {
        let mut change = None;
        if self.last_poll.is_none_or(|t| t.elapsed() >= TICK) {
            self.poll(player);
            self.last_poll = Some(Instant::now());
        }
        let cover = self
            .current_track()
            .and_then(Track::cover)
            .map(str::to_owned);
        self.cover.update(
            ctx,
            runtime,
            server,
            self.identity.map_or(0, |v| v.0),
            cover.as_deref(),
        );
        let error = self
            .error
            .as_deref()
            .or(self.snapshot.error.as_deref())
            .map(str::to_owned);
        let bar = egui::TopBottomPanel::bottom("musicfs-player")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(style::sidebar(ctx.style().visuals.dark_mode)))
            .show(ctx, |ui| {
                let queue_button = self.bar(ui, player, volume, last_volume, &mut change);
                if let Some(error) = error {
                    ui.horizontal(|ui| {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                        if self.snapshot.status == Status::Error
                            && ui.small_button(tr("重新播放")).clicked()
                        {
                            self.run(player, PlayerCommand::Play);
                        }
                    });
                }
                queue_button
            });
        if let Some(command) = self
            .queue
            .show(ctx, bar.response.rect, &bar.inner, &self.snapshot)
        {
            self.run(player, command);
        }
        self.windows.show(ctx, player, server, runtime);
        ctx.request_repaint_after(TICK);
        change
    }

    fn poll(&mut self, player: &Player) {
        let now = match player.presentation() {
            Ok(now) => now,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let identity = Some((now.instance, now.generation));
        if self.identity != identity
            || self
                .last_full
                .is_none_or(|t| t.elapsed() > Duration::from_secs(2))
        {
            if self.identity != identity {
                self.seek_draft = None;
            }
            match player.snapshot() {
                Ok(value) => {
                    self.snapshot = value;
                    self.identity = identity;
                    self.last_full = Some(Instant::now());
                }
                Err(error) => self.error = Some(error),
            }
        } else {
            self.snapshot.position_ms = now.position_ms;
            self.snapshot.duration_ms = now.duration_ms;
            // 初次解码或定位成功后可定位能力可能变化，需要更新完整快照。
            if self.snapshot.status != now.status {
                self.last_full = None;
            }
            self.snapshot.status = now.status;
            self.snapshot.error = now.error;
        }
    }

    fn run(&mut self, player: &Player, command: PlayerCommand) {
        match player.command(command) {
            Ok(value) => {
                self.snapshot = value;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        self.last_poll = None;
    }
}
