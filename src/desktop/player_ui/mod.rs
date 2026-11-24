use crate::i18n::tr;
mod bar;
mod controls;
pub(crate) mod cover;
mod lyrics;
mod windows;

use crate::ui::style;
use cover::Cover;
use eframe::egui::{self, Context};
use musicfs_player_core::{Player, PlayerCommand, PlayerSnapshot, Status, Track};
use musicfs_server_adapter::ServerSession;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use windows::Windows;

const TICK: Duration = Duration::from_millis(160);

#[derive(Default)]
pub struct PlayerUi {
    snapshot: PlayerSnapshot,
    identity: Option<(u64, u64)>,
    last_poll: Option<Instant>,
    last_full: Option<Instant>,
    error: Option<String>,
    seek_draft: Option<u64>,
    queue_open: bool,
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
    ) -> Option<f32> {
        let mut volume_change = None;
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
        egui::TopBottomPanel::bottom("musicfs-player")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(style::sidebar(ctx.style().visuals.dark_mode)))
            .show(ctx, |ui| {
                self.bar(ui, player, volume, last_volume, &mut volume_change);
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
            });
        if self.queue_open {
            self.queue(ctx, player);
        }
        self.windows.show(ctx, player, server, runtime);
        ctx.request_repaint_after(TICK);
        volume_change
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

    fn queue(&mut self, ctx: &Context, player: &Player) {
        let mut open = self.queue_open;
        let mut command = None;
        egui::Window::new(tr("播放队列"))
            .id(egui::Id::new("musicfs-queue"))
            .open(&mut open)
            .default_width(390.)
            .default_height(305.)
            .min_width(255.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.weak(
                        tr("{0} 首曲目").replace("{0}", &self.snapshot.queue.len().to_string()),
                    );
                    if ui
                        .add_enabled(
                            !self.snapshot.queue.is_empty(),
                            egui::Button::new(tr("清空队列")),
                        )
                        .clicked()
                    {
                        command = Some(PlayerCommand::Clear);
                    }
                });
                ui.separator();
                if self.snapshot.queue.is_empty() {
                    ui.weak(tr("队列为空"));
                }
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    24.,
                    self.snapshot.queue.len(),
                    |ui, range| {
                        for index in range {
                            let track = &self.snapshot.queue[index];
                            let active = self.snapshot.index == Some(index);
                            let artist = track.artists.join(" / ");
                            let label = if artist.is_empty() {
                                track.title.clone()
                            } else {
                                format!("{}   ·   {artist}", track.title)
                            };
                            ui.horizontal(|ui| {
                                ui.weak(format!("{:02}", index + 1));
                                if ui
                                    .add(egui::SelectableLabel::new(active, label))
                                    .on_hover_text(&track.title)
                                    .clicked()
                                {
                                    command = Some(PlayerCommand::Select { index });
                                }
                            });
                        }
                    },
                );
            });
        self.queue_open = open;
        if let Some(command) = command {
            self.run(player, command);
        }
    }
}
