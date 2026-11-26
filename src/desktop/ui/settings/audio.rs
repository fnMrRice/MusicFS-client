mod effects;
use super::{HostAction, Services};
use crate::{
    i18n::tr,
    model::{audio::AudioAction, Preferences},
    task::Task,
    ui::style,
};
use eframe::egui::{self, Ui};
use musicfs_player_core::{
    audio::{EffectKind, Layout, LocalDsp, OutputStatus, Processing, Routing},
    PlayerCommand,
};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct AudioSettingsUi {
    dsp: Option<LocalDsp>,
    routing: Option<Routing>,
    kind: Option<EffectKind>,
    device: Option<Result<u16, String>>,
    probe: Option<Task<u16>>,
    status: OutputStatus,
    inspected: Option<Instant>,
    error: Option<String>,
    saved_dsp: bool,
    saved_routing: bool,
}
impl AudioSettingsUi {
    pub(super) fn playback(
        &mut self,
        ui: &mut Ui,
        _svc: Services<'_>,
        prefs: &Preferences,
        action: &mut Option<HostAction>,
    ) {
        let audio = &prefs.audio;
        ui.heading(tr("播放"));
        ui.add_space(10.);
        let mut volume = (audio.volume * 100.).round();
        if volume_control(ui, &mut volume) {
            *action = Some(HostAction::Audio(AudioAction::Volume(volume / 100.)));
        }
        ui.separator();
        let mut fades = prefs.fades;
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut fades.enabled, tr("淡出淡入"));
            ui.add_enabled_ui(fades.enabled, |ui| {
                for (label, ms) in [
                    (tr("切歌"), &mut fades.track_ms),
                    (tr("暂停"), &mut fades.pause_ms),
                    (tr("跳转"), &mut fades.seek_ms),
                ] {
                    let label = ui.label(label);
                    ui.add(
                        egui::DragValue::new(ms)
                            .range(0..=2000)
                            .speed(5.)
                            .suffix(" ms"),
                    )
                    .labelled_by(label.id);
                }
            });
        });
        if fades != prefs.fades {
            *action = Some(HostAction::Fades(fades));
        }
        ui.separator();
        let mut level = audio.processing.auto_volume;
        if ui.checkbox(&mut level, tr("自动音量平衡")).changed() {
            *action = Some(HostAction::Audio(AudioAction::Level(level)));
        }
    }

    pub(super) fn local_dsp(
        &mut self,
        ui: &mut Ui,
        svc: Services<'_>,
        prefs: &Preferences,
        action: &mut Option<HostAction>,
    ) {
        let saved = &prefs.audio.processing;
        let dsp = self.dsp.get_or_insert_with(|| saved.dsp.clone());
        let kind = self.kind.get_or_insert(EffectKind::Gain);
        ui.heading(tr("本地音效"));
        let mut changed = ui.checkbox(&mut dsp.enabled, tr("启用本地音效")).changed();
        if !dsp.enabled {
            ui.weak(tr("本地音效已旁路，保留当前效果和参数。"));
        }
        changed |= effects::edit(ui, dsp, kind);
        if changed {
            self.saved_dsp = false;
        }
        ui.horizontal_wrapped(|ui| {
            let dirty = *dsp != saved.dsp;
            if ui
                .add_enabled(dirty, egui::Button::new(tr("撤销更改")))
                .clicked()
            {
                *dsp = saved.dsp.clone();
                changed = true;
                self.saved_dsp = false;
            }
            if ui
                .add_enabled(dirty, egui::Button::new(tr("保存")))
                .clicked()
            {
                *action = Some(HostAction::Audio(AudioAction::SaveDsp(dsp.clone())));
                self.saved_dsp = true;
            }
            status_message(ui, dirty, self.saved_dsp);
        });
        ui.add_space(14.);
        ui.separator();
        self.inspect(ui, svc, false);
        let routing = self.routing.get_or_insert(saved.routing);
        ui.heading(tr("输出设置"));
        let before = *routing;
        let max = self
            .status
            .max_channels
            .or_else(|| self.device.as_ref().and_then(|r| r.as_ref().ok().copied()));
        egui::ComboBox::from_label(tr("输出声道"))
            .selected_text(tr(routing.layout.name()))
            .show_ui(ui, |ui| {
                for layout in [
                    Layout::Source,
                    Layout::Mono,
                    Layout::Stereo,
                    Layout::Surround51,
                    Layout::Surround71,
                ] {
                    let enabled =
                        layout == Layout::Source || max.is_some_and(|n| layout.channels(n) <= n);
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.selectable_value(&mut routing.layout, layout, tr(layout.name()))
                    });
                }
            });
        ui.checkbox(&mut routing.expand_stereo, tr("立体声环绕扩展"));
        ui.weak(tr(
            "输出为 5.1 或 7.1 时，将双声道音乐扩展为模拟环绕；关闭时仅保留原有声道。",
        ));
        if self.status.limited {
            ui.weak(tr(
                "当前设备无法使用所选布局，已自动使用支持的声道数；保存的偏好不会改变。",
            ));
        }
        if before != *routing {
            self.saved_routing = false;
        }
        ui.horizontal_wrapped(|ui| {
            let dirty = *routing != saved.routing;
            if ui
                .add_enabled(dirty, egui::Button::new(tr("撤销更改")))
                .clicked()
            {
                *routing = saved.routing;
                self.saved_routing = false;
            }
            if ui
                .add_enabled(dirty, egui::Button::new(tr("保存")))
                .clicked()
            {
                *action = Some(HostAction::Audio(AudioAction::SaveRouting(*routing)));
                self.saved_routing = true;
            }
            status_message(ui, dirty, self.saved_routing);
        });
        changed |= before != *routing;
        if changed {
            let settings = Processing {
                dsp: self.dsp.clone().unwrap_or_default(),
                routing: *routing,
                auto_volume: saved.auto_volume,
            };
            match svc.player.command(PlayerCommand::Processing { settings }) {
                Ok(snapshot) => {
                    self.status = snapshot.output;
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        ui.add_space(6.);
        egui::Grid::new("channel-status")
            .spacing([16., 4.])
            .show(ui, |ui| {
                for (key, value) in [
                    ("设备最多支持", max),
                    ("音源声道", self.status.source_channels),
                    ("当前输出", self.status.output_channels),
                ] {
                    ui.weak(tr(key));
                    ui.label(
                        value
                            .map(|v| tr("{count} 声道").replace("{count}", &v.to_string()))
                            .unwrap_or_else(|| {
                                tr(if key == "设备最多支持" {
                                    "未检测到"
                                } else {
                                    "播放后显示"
                                })
                                .into()
                            }),
                    );
                    ui.end_row();
                }
            });
        if let Some(error) = self
            .error
            .as_ref()
            .or_else(|| self.device.as_ref().and_then(|v| v.as_ref().err()))
        {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if ui
            .add_enabled(self.probe.is_none(), egui::Button::new(tr("重新检测")))
            .clicked()
        {
            self.inspect(ui, svc, true);
        }
    }
    fn inspect(&mut self, ui: &Ui, svc: Services<'_>, refresh: bool) {
        if let Some(result) = self.probe.as_mut().and_then(Task::poll) {
            self.probe = None;
            self.device = Some(result);
        }
        if self.probe.is_none() && (refresh || self.device.is_none()) {
            self.probe = Some(Task::spawn(svc.runtime, ui.ctx(), async {
                tokio::task::spawn_blocking(musicfs_player_core::device_channels)
                    .await
                    .map_err(|e| e.to_string())?
            }));
        }
        if refresh
            || self
                .inspected
                .is_none_or(|t| t.elapsed() > Duration::from_millis(500))
        {
            if let Ok(snapshot) = svc.player.snapshot() {
                self.status = snapshot.output;
            }
            self.inspected = Some(Instant::now());
        }
    }
}

fn volume_control(ui: &mut Ui, volume: &mut f32) -> bool {
    ui.scope(|ui| {
        let dark = ui.visuals().dark_mode;
        let accent = style::accent(ui);
        ui.spacing_mut().slider_width = (ui.available_width() - 128.).clamp(72., 230.);
        ui.spacing_mut().slider_rail_height = 6.;
        ui.spacing_mut().interact_size.y = 24.;
        let visuals = ui.visuals_mut();
        visuals.selection.bg_fill = accent;
        visuals.widgets.inactive.bg_fill = style::muted(dark);
        visuals.widgets.inactive.fg_stroke.width = 2.;
        for state in [&mut visuals.widgets.hovered, &mut visuals.widgets.active] {
            state.bg_fill = accent;
            state.fg_stroke.width = 2.;
        }
        ui.add(
            egui::Slider::new(volume, 0.0..=100.)
                .trailing_fill(true)
                .fixed_decimals(0)
                .suffix("%")
                .text(tr("音量")),
        )
        .changed()
    })
    .inner
}

fn status_message(ui: &mut Ui, dirty: bool, saved: bool) {
    ui.weak(tr(if dirty {
        "有未保存的更改"
    } else if saved {
        "已保存"
    } else {
        "调整时实时试听"
    }));
}
