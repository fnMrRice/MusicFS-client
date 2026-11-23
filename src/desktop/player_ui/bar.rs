use super::{lyrics::time, PlayerUi};
use crate::{i18n::tr, icons, ui::style};
use eframe::egui::{self, Align, Color32, Layout, Rect, RichText, Ui, UiBuilder};
use musicfs_player_core::{Player, PlayerCommand, Status};

impl PlayerUi {
    pub(super) fn bar(
        &mut self,
        ui: &mut Ui,
        player: &Player,
        volume: f32,
        last_volume: f32,
        change: &mut Option<f32>,
    ) {
        let width = ui.available_width();
        let (bar, _) = ui.allocate_exact_size(egui::vec2(width, 105.), egui::Sense::hover());
        self.timeline(ui, bar, player);
        let narrow = width <= 850.;
        let compact = width <= 1150.;
        let padding = if compact { 18. } else { 27. };
        let row = bar
            .shrink2(egui::vec2(padding, 0.))
            .with_min_y(bar.top() + 7.);
        let gap = if narrow {
            2.
        } else if compact {
            8.
        } else {
            18.
        };
        let middle_width = 36. * 2. + 52. + 2. * gap + if narrow { 0. } else { 93. + gap };
        let middle = Rect::from_center_size(row.center(), egui::vec2(middle_width, row.height()));
        let mut info = ui.new_child(
            UiBuilder::new()
                .max_rect(row.with_max_x(middle.left() - 8.))
                .layout(Layout::left_to_right(Align::Center)),
        );
        self.track_info(&mut info, narrow);
        let mut transport = ui.new_child(
            UiBuilder::new()
                .max_rect(middle)
                .layout(Layout::left_to_right(Align::Center)),
        );
        transport.spacing_mut().item_spacing.x = gap;
        self.transport(&mut transport, player, narrow);
        let mut extras = ui.new_child(
            UiBuilder::new()
                .max_rect(row.with_min_x(middle.right() + 6.))
                .layout(Layout::right_to_left(Align::Center)),
        );
        extras.spacing_mut().item_spacing.x = if compact { 2. } else { 7. };
        self.extras(&mut extras, volume, last_volume, change, compact, narrow);
    }

    fn timeline(&mut self, ui: &mut Ui, bar: Rect, player: &Player) {
        let hit = bar.with_max_y(bar.top() + 13.);
        let mut region = ui.new_child(
            UiBuilder::new()
                .max_rect(hit)
                .layout(Layout::left_to_right(Align::Center)),
        );
        // 保留原生滑块的键盘和辅助功能；轨道与指示器由下方按网页样式绘制。
        region.set_clip_rect(hit.intersect(ui.clip_rect()));
        region.set_opacity(0.);
        region.spacing_mut().slider_width = hit.width();
        region.spacing_mut().interact_size.y = hit.height();
        let range = self
            .snapshot
            .seek_range_ms
            .filter(|(start, end)| self.snapshot.seekable && end > start);
        let (start, end) = range.unwrap_or((0, self.snapshot.duration_ms.unwrap_or(0)));
        let mut target = self
            .seek_draft
            .unwrap_or(self.snapshot.position_ms)
            .clamp(start, end);
        let response = region
            .add_enabled(
                range.is_some(),
                egui::Slider::new(&mut target, start..=end.max(start.saturating_add(1)))
                    .step_by(100.)
                    .show_value(false),
            )
            .on_hover_text(tr("曲目进度"));
        response.widget_info(|| {
            egui::WidgetInfo::slider(range.is_some(), target as f64, tr("曲目进度"))
        });
        if response.changed() {
            self.seek_draft = Some(target);
        }
        if (response.drag_stopped() || (response.changed() && !response.dragged()))
            && self.seek_draft.take().is_some()
        {
            self.run(
                player,
                PlayerCommand::Seek {
                    position_ms: target,
                },
            );
        }
        let painter = ui.painter();
        let dark = ui.visuals().dark_mode;
        painter.rect_filled(hit, 0., style::sidebar(dark));
        let active =
            range.is_some() && (response.hovered() || response.has_focus() || response.dragged());
        let y = hit.top() + 3.;
        let thumb_inset = 6.;
        let fraction = if end > start {
            (target - start) as f32 / (end - start) as f32
        } else {
            0.
        };
        let x = egui::lerp(
            (hit.left() + thumb_inset)..=(hit.right() - thumb_inset),
            fraction,
        );
        let height: f32 = if active { 5. } else { 2. };
        painter.hline(
            hit.x_range(),
            y,
            egui::Stroke::new(
                height,
                if dark {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(198, 204, 210)
                },
            ),
        );
        if range.is_some() {
            painter.hline(
                hit.left()..=x,
                y,
                egui::Stroke::new(height, style::accent(ui)),
            );
        }
        let ink = if range.is_some() {
            style::accent(ui)
        } else {
            style::muted(dark)
        };
        if active {
            painter.circle_filled(egui::pos2(x, y.max(bar.top() + 6.)), 6., ink);
        } else {
            painter.rect_filled(
                Rect::from_center_size(egui::pos2(x, y), egui::vec2(2., 6.)),
                0.,
                ink,
            );
        }
    }

    fn track_info(&self, ui: &mut Ui, narrow: bool) {
        ui.spacing_mut().item_spacing.x = 13.;
        let side = if narrow { 44. } else { 56. };
        let (art, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
        if let Some(texture) = &self.cover.texture {
            egui::Image::new((texture.id(), art.size()))
                .corner_radius(6)
                .paint_at(ui, art);
        } else {
            style::vinyl(ui, art);
        }
        let track = self.current_track();
        let artist = track
            .map(|t| {
                if t.artists.is_empty() {
                    tr("未知歌手").into()
                } else {
                    t.artists.join(" / ")
                }
            })
            .unwrap_or_else(|| tr("暂无作者").into());
        let label = track
            .map(|t| t.title.as_str())
            .unwrap_or_else(|| tr("暂无音频"));
        let output = self
            .snapshot
            .output
            .output_channels
            .map(|n| tr("{0} 声道").replace("{0}", &n.to_string()))
            .unwrap_or_else(|| tr("暂无编码信息").into());
        let height = ui.text_style_height(&egui::TextStyle::Body) + 35.;
        let max = ui.available_rect_before_wrap();
        let bounds = Rect::from_center_size(max.center(), egui::vec2(max.width().max(0.), height));
        let mut text = ui.new_child(
            UiBuilder::new()
                .max_rect(bounds)
                .layout(Layout::top_down(Align::Min)),
        );
        text.spacing_mut().item_spacing.y = 2.;
        for line in [
            RichText::new(label).font(style::bold(&text, 14.)).strong(),
            RichText::new(artist).size(12.).weak(),
            RichText::new(output).size(10.).weak(),
        ] {
            text.add(egui::Label::new(line).truncate());
        }
    }

    fn transport(&mut self, ui: &mut Ui, player: &Player, narrow: bool) {
        let index = self.snapshot.index;
        let prev = ui
            .add_enabled_ui(index.is_some_and(|i| i > 0), |ui| {
                icons::button(ui, "previous", tr("上一首"), 36., 22.)
            })
            .inner;
        if prev.clicked() {
            self.run(player, PlayerCommand::Previous);
        }
        let playing = matches!(self.snapshot.status, Status::Playing | Status::Loading);
        if super::controls::play_button(ui, playing, index.is_some(), 52.).clicked() {
            self.run(player, PlayerCommand::Toggle);
        }
        let next = ui
            .add_enabled_ui(
                index.is_some_and(|i| i + 1 < self.snapshot.queue.len()),
                |ui| icons::button(ui, "next", tr("下一首"), 36., 22.),
            )
            .inner;
        if next.clicked() {
            self.run(player, PlayerCommand::Next);
        }
        if !narrow {
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{} / {}",
                        time(self.seek_draft.unwrap_or(self.snapshot.position_ms)),
                        self.snapshot
                            .duration_ms
                            .map(time)
                            .unwrap_or_else(|| "—".into())
                    ))
                    .size(11.)
                    .weak(),
                )
                .truncate(),
            );
        }
    }

    fn extras(
        &mut self,
        ui: &mut Ui,
        mut volume: f32,
        last_volume: f32,
        change: &mut Option<f32>,
        compact: bool,
        narrow: bool,
    ) {
        let slider_width = if narrow {
            45.
        } else if compact {
            54.
        } else {
            76.
        };
        ui.spacing_mut().slider_width = slider_width;
        ui.spacing_mut().interact_size.y = 18.;
        let slider = ui
            .scope(|ui| {
                ui.set_opacity(0.);
                ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false))
            })
            .inner;
        slider.widget_info(|| egui::WidgetInfo::slider(true, volume as f64, tr("音量")));
        let bounds = slider.rect.shrink2(egui::vec2(6., 0.));
        let x = egui::lerp(bounds.x_range(), volume);
        let y = bounds.center().y;
        let rail = style::muted(ui.visuals().dark_mode).gamma_multiply(0.65);
        let tint = style::accent(ui);
        ui.painter()
            .hline(bounds.x_range(), y, egui::Stroke::new(3.0_f32, rail));
        ui.painter()
            .hline(bounds.left()..=x, y, egui::Stroke::new(3.0_f32, tint));
        ui.painter().circle_filled(
            egui::pos2(x, y),
            if slider.has_focus() { 7. } else { 5.5 },
            tint,
        );
        if slider
            .on_hover_text(format!("{} {:.0}%", tr("音量"), volume * 100.))
            .changed()
        {
            *change = Some(volume);
        }
        let muted = volume == 0.;
        if icons::button(
            ui,
            if muted { "volume-muted" } else { "volume" },
            tr(if muted { "取消静音" } else { "静音" }),
            36.,
            21.,
        )
        .clicked()
        {
            *change = Some(if muted { last_volume } else { 0. });
        }
        if icons::button(ui, "lyrics", tr("桌面歌词"), 36., 21.).clicked() {
            self.windows.open_lyrics(ui.ctx());
        }
        if icons::button(ui, "queue", tr("播放队列"), 36., 21.).clicked() {
            self.queue_open = !self.queue_open;
        }
        if icons::button(ui, "mini", tr("迷你播放器"), 36., 21.).clicked() {
            self.windows.open_mini(ui.ctx());
        }
    }
}
