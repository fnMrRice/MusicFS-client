use super::controls::{heading, icon_button, Glyph, ACCENT};
use super::lyrics::{time, Lyrics};
use crate::task::Task;
use eframe::egui::{self, Context, RichText, ViewportBuilder, ViewportCommand, ViewportId};
use musicfs_player_core::{Player, PlayerCommand, Presentation, Status};
use musicfs_server_adapter::ServerSession;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;

#[derive(Default)]
pub struct Windows {
    mini: Arc<AtomicBool>,
    words: Arc<AtomicBool>,
    lyrics: Arc<Mutex<LyricsState>>,
}

impl Windows {
    pub fn open_mini(&self, ctx: &Context) {
        open(ctx, &self.mini, "musicfs-mini");
    }
    pub fn open_lyrics(&self, ctx: &Context) {
        open(ctx, &self.words, "musicfs-lyrics");
    }

    pub fn show(&self, ctx: &Context, player: &Player, server: &ServerSession, runtime: &Handle) {
        if self.mini.load(Ordering::Relaxed) {
            let visible = self.mini.clone();
            let player = player.clone();
            ctx.show_viewport_deferred(
                ViewportId::from_hash_of("musicfs-mini"),
                viewport("MusicFS · 迷你播放器", [410., 146.], [325., 140.]),
                move |ctx, _| {
                    if !active(ctx, &visible) {
                        return;
                    }
                    egui::CentralPanel::default().show(ctx, |ui| match player.presentation() {
                        Ok(now) => mini(ui, &player, &now, &visible),
                        Err(error) => {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
                    ctx.request_repaint_after(Duration::from_millis(170));
                },
            );
        }
        if self.words.load(Ordering::Relaxed) {
            let visible = self.words.clone();
            let player = player.clone();
            let lyrics = self.lyrics.clone();
            let server = server.clone();
            let runtime = runtime.clone();
            ctx.show_viewport_deferred(
                ViewportId::from_hash_of("musicfs-lyrics"),
                viewport("MusicFS · 桌面歌词", [640., 232.], [350., 170.]),
                move |ctx, _| {
                    if !active(ctx, &visible) {
                        return;
                    }
                    egui::CentralPanel::default().show(ctx, |ui| match player.presentation() {
                        Ok(now) => {
                            compact_header(ui, &now, &visible);
                            ui.separator();
                            if let Ok(mut lyrics) = lyrics.lock() {
                                lyrics.show(ui, &now, &runtime, &server);
                            }
                        }
                        Err(error) => {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
                    ctx.request_repaint_after(Duration::from_millis(150));
                },
            );
        }
    }
}

fn viewport(title: &str, size: [f32; 2], min: [f32; 2]) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(min)
        .with_always_on_top()
}

fn open(ctx: &Context, visible: &AtomicBool, id: &'static str) {
    if visible.swap(true, Ordering::Relaxed) {
        let id = ViewportId::from_hash_of(id);
        ctx.send_viewport_cmd_to(id, ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
    }
}

fn active(ctx: &Context, visible: &AtomicBool) -> bool {
    if ctx.input(|i| i.viewport().close_requested()) {
        visible.store(false, Ordering::Relaxed);
        ctx.request_repaint_of(ViewportId::ROOT);
    }
    visible.load(Ordering::Relaxed)
}

fn compact_header(ui: &mut egui::Ui, now: &Presentation, visible: &AtomicBool) {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 76.).max(60.);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 43.),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                heading(ui, now.title.as_deref().unwrap_or("尚未播放"), &now.artists);
            },
        );
        if ui.button("主窗口").on_hover_text("返回主窗口").clicked() {
            ui.ctx()
                .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Minimized(false));
            ui.ctx()
                .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
            visible.store(false, Ordering::Relaxed);
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            ui.ctx().request_repaint_of(ViewportId::ROOT);
        }
    });
}

#[cfg(test)]
#[path = "../../../tests/native/windows.rs"]
mod tests;

fn mini(ui: &mut egui::Ui, player: &Player, now: &Presentation, visible: &AtomicBool) {
    compact_header(ui, now, visible);
    let enabled = now.title.is_some();
    let playing = matches!(now.status, Status::Playing | Status::Loading);
    let error_key = egui::Id::new("mini-playback-error");
    ui.horizontal(|ui| {
        let mut action = None;
        if icon_button(ui, Glyph::Previous, "上一首", enabled, false).clicked() {
            action = Some(PlayerCommand::Previous);
        }
        if icon_button(
            ui,
            if playing { Glyph::Pause } else { Glyph::Play },
            if playing { "暂停" } else { "播放" },
            enabled,
            true,
        )
        .clicked()
        {
            action = Some(PlayerCommand::Toggle);
        }
        if icon_button(ui, Glyph::Next, "下一首", now.has_next, false).clicked() {
            action = Some(PlayerCommand::Next);
        }
        if let Some(command) = action {
            let error = player.command(command).err().unwrap_or_default();
            ui.data_mut(|data| data.insert_temp(error_key, error));
            ui.ctx().request_repaint_of(ViewportId::ROOT);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small(format!(
                "{} / {}",
                time(now.position_ms),
                now.duration_ms.map(time).unwrap_or_else(|| "–:––".into())
            ));
        });
    });
    ui.add_space(7.);
    let progress = now
        .duration_ms
        .filter(|ms| *ms > 0)
        .map_or(0., |ms| now.position_ms as f32 / ms as f32);
    ui.add(
        egui::ProgressBar::new(progress)
            .fill(ACCENT)
            .desired_height(4.),
    );
    let error = ui.data(|data| data.get_temp::<String>(error_key));
    if let Some(error) = now
        .error
        .as_ref()
        .or(error.as_ref())
        .filter(|s| !s.is_empty())
    {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
}

#[derive(Default)]
struct LyricsState {
    key: Option<(u64, u64, String)>,
    request: Option<Task<Lyrics>>,
    result: Option<Result<Lyrics, String>>,
}

impl LyricsState {
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        now: &Presentation,
        runtime: &Handle,
        server: &ServerSession,
    ) {
        let key = now
            .source_id
            .as_ref()
            .map(|id| (now.instance, now.generation, id.clone()));
        if key != self.key {
            self.key = key;
            self.result = None;
            self.request = now.source_id.clone().map(|id| {
                let server = server.clone();
                Task::spawn(runtime, ui.ctx(), async move {
                    server.lyrics(&id).await.map(Lyrics::parse)
                })
            });
        }
        if let Some(result) = self.request.as_mut().and_then(Task::poll) {
            self.result = Some(result);
            self.request = None;
        }
        ui.add_space(10.);
        match self.result.as_ref() {
            Some(Ok(Lyrics::Timed(lines))) => {
                let next = lines.partition_point(|l| l.time_ms <= now.position_ms);
                let current = next.checked_sub(1).map(|i| &lines[i]);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        if let Some(line) = current.filter(|line| !line.parts.is_empty()) {
                            for text in &line.parts {
                                ui.label(RichText::new(text.as_ref()).size(27.).strong());
                            }
                        } else {
                            ui.label(
                                RichText::new(if current.is_some() {
                                    "间奏"
                                } else {
                                    "前奏"
                                })
                                .size(27.)
                                .strong(),
                            );
                        }
                        if let Some(line) = lines.get(next) {
                            ui.add_space(7.);
                            for text in &line.parts {
                                ui.weak(text.as_ref());
                            }
                        }
                    })
                });
            }
            Some(Ok(Lyrics::Plain(raw))) => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new(raw).size(18.));
                });
            }
            Some(Ok(Lyrics::Empty)) => {
                ui.vertical_centered(|ui| ui.weak("当前曲目暂无歌词"));
            }
            Some(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
                if ui.button("重试读取歌词").clicked() {
                    self.key = None;
                }
            }
            None => {
                ui.vertical_centered(|ui| {
                    ui.weak(if now.source_id.is_some() {
                        "正在读取歌词…"
                    } else {
                        "等待播放"
                    })
                });
            }
        }
    }
}
