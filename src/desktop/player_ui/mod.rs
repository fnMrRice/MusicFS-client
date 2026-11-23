use crate::i18n::tr;
mod controls;
pub(crate) mod cover;
mod lyrics;
mod windows;

use controls::{glyph, heading, icon_button, Glyph, ACCENT};
use cover::Cover;
use eframe::egui::{self, Color32, Context, RichText, Ui};
use lyrics::time;
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
        let cover = self.current_track().and_then(|t| t.cover_object_id.clone());
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
            .frame(
                egui::Frame::default()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(16, 7)),
            )
            .show(ctx, |ui| {
                self.timeline(ui, player);
                ui.add_space(4.);
                self.toolbar(ui, player, volume, last_volume, &mut volume_change);
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

    fn timeline(&mut self, ui: &mut Ui, player: &Player) {
        let duration = self.snapshot.duration_ms.unwrap_or(0);
        if let Some((start, end)) = self
            .snapshot
            .seek_range_ms
            .filter(|_| self.snapshot.seekable)
        {
            let mut target = self
                .seek_draft
                .unwrap_or(self.snapshot.position_ms)
                .clamp(start, end);
            let response = ui
                .scope(|ui| {
                    ui.spacing_mut().slider_width = ui.available_width();
                    ui.add(
                        egui::Slider::new(&mut target, start..=end)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .on_hover_text(format!(
                        "{} · {}",
                        tr("播放进度"),
                        time(target)
                    ))
                })
                .inner;
            if response.changed() {
                self.seek_draft = Some(target);
            }
            if (response.drag_stopped() || (response.changed() && !response.dragged()))
                && self.seek_draft.is_some()
            {
                self.seek_draft = None;
                self.run(
                    player,
                    PlayerCommand::Seek {
                        position_ms: target,
                    },
                );
            }
        } else {
            let progress = if duration == 0 {
                0.
            } else {
                self.snapshot.position_ms as f32 / duration as f32
            };
            ui.add_space(6.);
            ui.add(
                egui::ProgressBar::new(progress)
                    .fill(ACCENT)
                    .desired_height(4.),
            )
            .on_hover_text(tr("播放进度"));
            ui.add_space(6.);
        }
    }

    fn toolbar(
        &mut self,
        ui: &mut Ui,
        player: &Player,
        mut volume: f32,
        last_volume: f32,
        volume_change: &mut Option<f32>,
    ) {
        let width = ui.available_width();
        let narrow = width < 790.;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = if narrow { 3. } else { 8. };
            let (art, _) = ui.allocate_exact_size(
                egui::Vec2::splat(if narrow { 39. } else { 51. }),
                egui::Sense::hover(),
            );
            if let Some(texture) = &self.cover.texture {
                ui.painter().image(
                    texture.id(),
                    art,
                    egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
            } else {
                ui.painter()
                    .rect_filled(art, 8., ui.visuals().widgets.inactive.bg_fill);
                glyph(
                    ui,
                    art.center(),
                    Glyph::Record,
                    ui.visuals().weak_text_color(),
                    1.35,
                );
            }
            let track_width = if narrow {
                width * 0.34
            } else {
                width * 0.5 - 72.
            };
            let track_width = (track_width - art.width()).max(76.);
            ui.allocate_ui_with_layout(
                egui::vec2(track_width, art.height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(track_width);
                    if let Some(track) = self.current_track() {
                        heading(ui, &track.title, &track.artists);
                    } else {
                        ui.label(RichText::new(tr("暂无音频")).size(16.).strong());
                        ui.small(tr("尚未选择曲目"));
                    }
                },
            );
            let index = self.snapshot.index;
            let playing = matches!(self.snapshot.status, Status::Playing | Status::Loading);
            if icon_button(
                ui,
                Glyph::Previous,
                tr("上一首"),
                index.is_some_and(|i| i > 0),
                false,
            )
            .clicked()
            {
                self.run(player, PlayerCommand::Previous);
            }
            if icon_button(
                ui,
                if playing { Glyph::Pause } else { Glyph::Play },
                if playing { tr("暂停") } else { tr("播放") },
                index.is_some(),
                true,
            )
            .clicked()
            {
                self.run(player, PlayerCommand::Toggle);
            }
            if icon_button(
                ui,
                Glyph::Next,
                tr("下一首"),
                index.is_some_and(|i| i + 1 < self.snapshot.queue.len()),
                false,
            )
            .clicked()
            {
                self.run(player, PlayerCommand::Next);
            }
            if !narrow {
                ui.small(format!(
                    "{} / {}",
                    time(self.seek_draft.unwrap_or(self.snapshot.position_ms)),
                    self.snapshot
                        .duration_ms
                        .map(time)
                        .unwrap_or_else(|| "–:––".into())
                ));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, Glyph::Queue, tr("播放队列"), true, false).clicked() {
                    self.queue_open = !self.queue_open;
                }
                if icon_button(ui, Glyph::Lyrics, tr("桌面歌词"), true, false).clicked() {
                    self.windows.open_lyrics(ui.ctx());
                }
                if icon_button(ui, Glyph::Mini, tr("迷你播放器"), true, false).clicked() {
                    self.windows.open_mini(ui.ctx());
                }
                if !narrow {
                    ui.spacing_mut().slider_width = 66.;
                    ui.spacing_mut().interact_size.y = 16.;
                    if ui
                        .add_sized(
                            [66., 16.],
                            egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false),
                        )
                        .on_hover_text(tr("音量"))
                        .changed()
                    {
                        *volume_change = Some(volume);
                    }
                }
                if icon_button(
                    ui,
                    if volume == 0. {
                        Glyph::Muted
                    } else {
                        Glyph::Volume
                    },
                    if volume == 0. {
                        tr("取消静音")
                    } else {
                        tr("静音")
                    },
                    true,
                    false,
                )
                .clicked()
                {
                    *volume_change = Some(if volume > 0. { 0. } else { last_volume });
                }
            });
        });
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
