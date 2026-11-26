use super::{AudioAction, HostAction};
use crate::{
    i18n::tr,
    task::Task,
    ui::style::{self, ComboBoxMotion},
};
use eframe::egui::{self, Ui};
use musicfs_player_core::{device_channels, output_devices, OutputDeviceInfo};
use tokio::runtime::Handle;

#[cfg(test)]
#[path = "../../../../../tests/native/audio_settings.rs"]
mod tests;

struct DeviceList {
    entries: Vec<OutputDeviceInfo>,
    default_channels: Option<u16>,
}

impl DeviceList {
    fn inspect(
        entries: Vec<OutputDeviceInfo>,
        probe_default: impl FnOnce() -> Result<u16, String>,
    ) -> Self {
        // 某些后端的默认输出是虚拟设备，枚举列表中可能没有对应项。
        let default_channels = entries
            .iter()
            .find(|d| d.is_default)
            .and_then(|d| d.max_channels)
            .or_else(|| probe_default().ok());
        Self {
            entries,
            default_channels,
        }
    }
}

#[derive(Default)]
pub(super) struct DeviceSelector {
    devices: Option<DeviceList>,
    probe: Option<Task<DeviceList>>,
    error: Option<String>,
}

impl DeviceSelector {
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        runtime: &Handle,
        selected: Option<&str>,
        action: &mut Option<HostAction>,
    ) -> Option<u16> {
        if let Some(result) = self.probe.as_mut().and_then(Task::poll) {
            self.probe = None;
            match result {
                Ok(devices) => {
                    self.devices = Some(devices);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }

        let devices = self
            .devices
            .as_ref()
            .map(|list| list.entries.as_slice())
            .unwrap_or_default();
        let default = devices.iter().find(|d| d.is_default);
        let default_label = match default {
            Some(device) => tr("系统默认设备（{0}）").replace("{0}", &device.name),
            None => tr("系统默认设备").to_owned(),
        };
        let current = selected.and_then(|id| devices.iter().find(|d| d.id == id));
        let label: &str = match (selected, current) {
            (None, _) => &default_label,
            (Some(_), Some(device)) => &device.name,
            (Some(_), None) if self.devices.is_some() => tr("已选设备暂不可用"),
            (Some(_), None) => tr("已选设备"),
        };
        let mut choice = selected;
        let refresh = ui
            .horizontal_wrapped(|ui| {
                let caption = ui.label(tr("输出设备"));
                egui::ComboBox::from_id_salt("audio-output-device")
                    .selected_text(label)
                    .width(ui.available_width().clamp(80., 280.))
                    .truncate()
                    .show_animated(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        style::selectable_value(ui, &mut choice, None, &default_label);
                        for device in devices {
                            style::selectable_value(
                                ui,
                                &mut choice,
                                Some(device.id.as_str()),
                                &device.name,
                            );
                        }
                    })
                    .response
                    .labelled_by(caption.id);
                style::add_button(
                    ui,
                    self.probe.is_none(),
                    egui::Button::new(tr("刷新设备列表")),
                )
                .clicked()
            })
            .inner;
        if choice != selected {
            *action = Some(HostAction::Audio(AudioAction::OutputDevice(
                choice.map(str::to_owned),
            )));
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.probe.is_none() && (refresh || self.devices.is_none() && self.error.is_none()) {
            self.probe = Some(Task::spawn(runtime, ui.ctx(), async {
                tokio::task::spawn_blocking(|| {
                    output_devices().map(|entries| DeviceList::inspect(entries, device_channels))
                })
                .await
                .map_err(|e| e.to_string())?
            }));
        }
        match selected {
            Some(_) => current.and_then(|device| device.max_channels),
            None => self.devices.as_ref().and_then(|list| list.default_channels),
        }
    }
}
