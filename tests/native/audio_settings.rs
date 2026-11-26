use super::super::{frames, AudioSettingsUi};
use super::{AudioAction, DeviceList, HostAction, OutputDeviceInfo};
use crate::{i18n::tr, model::Preferences};
use eframe::egui::{
    self,
    accesskit::{Role, TreeUpdate},
    Event, FullOutput, Pos2,
};
use musicfs_player_core::{audio::Layout, Player};
use tokio::runtime::Runtime;

struct Panel {
    ctx: egui::Context,
    runtime: Runtime,
    player: Player,
    settings: AudioSettingsUi,
    prefs: Preferences,
}

impl Panel {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut settings = AudioSettingsUi::default();
        settings.devices.devices = Some(DeviceList::inspect(
            vec![
                OutputDeviceInfo {
                    id: "a".into(),
                    name: "Speakers".into(),
                    is_default: true,
                    max_channels: Some(2),
                },
                OutputDeviceInfo {
                    id: "b".into(),
                    name: "Receiver".into(),
                    is_default: false,
                    max_channels: Some(8),
                },
            ],
            || panic!("The listed default already has channel information"),
        ));
        Self {
            ctx,
            runtime: tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
            player: Player::new(|_| Err("No media loaded.".into())).unwrap(),
            settings,
            prefs: Preferences::default(),
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> (FullOutput, Option<HostAction>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(780., 780.),
            )),
            events,
            ..Default::default()
        };
        let mut action = None;
        let output = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.settings.local_dsp(
                    ui,
                    &self.player,
                    self.runtime.handle(),
                    &self.prefs,
                    &mut action,
                )
            });
        });
        (output, action)
    }

    fn tree(&mut self) -> TreeUpdate {
        self.frame(vec![]);
        self.frame(vec![])
            .0
            .platform_output
            .accesskit_update
            .unwrap()
    }

    fn click(&mut self, role: Role, label: &str) -> Option<HostAction> {
        let tree = self.tree();
        let label_id = tree.nodes.iter().find_map(|(id, n)| {
            (n.role() == Role::Label && n.value() == Some(label)).then_some(id)
        });
        let node = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| {
                n.role() == role
                    && !n.is_disabled()
                    && (n.label() == Some(label)
                        || label_id.is_some_and(|id| n.labelled_by().contains(id)))
            })
            .unwrap_or_else(|| panic!("Missing enabled {role:?}: {label}"));
        let bounds = node.bounds().unwrap();
        let pos = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.) as f32,
            ((bounds.y0 + bounds.y1) / 2.) as f32,
        );
        let pointer = |pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(pos), pointer(true)]);
        self.frame(vec![pointer(false)]).1
    }

    fn select(&mut self, field: &str, value: &str) -> Option<HostAction> {
        self.click(Role::ComboBox, tr(field));
        self.click(Role::Button, value)
    }
}

#[test]
fn dsp_buffer_changes_can_be_saved_and_reverted() {
    let mut panel = Panel::new();
    panel.select("DSP 缓冲区大小", &frames(1024));
    let Some(HostAction::Audio(AudioAction::SaveDsp(dsp))) = panel.click(Role::Button, tr("保存"))
    else {
        panic!("Expected to save the DSP settings");
    };
    assert_eq!(dsp.buffer_frames, 1024);
    panel.prefs.audio.processing.dsp = dsp;
    panel.select("DSP 缓冲区大小", &frames(128));
    assert_eq!(panel.settings.dsp.as_ref().unwrap().buffer_frames, 128);
    panel.click(Role::Button, tr("撤销更改"));
    assert_eq!(panel.settings.dsp.as_ref().unwrap().buffer_frames, 1024);
}

#[test]
fn selecting_an_output_and_returning_to_default_updates_channels() {
    let mut panel = Panel::new();
    panel.prefs.audio.output_device = Some("disconnected".into());
    let Some(HostAction::Audio(AudioAction::OutputDevice(device))) =
        panel.select("输出设备", "Receiver")
    else {
        panic!("Expected to select the receiver");
    };
    assert_eq!(device.as_deref(), Some("b"));
    panel.prefs.audio.output_device = device;
    let channels = |n: &str| tr("{count} 声道").replace("{count}", n);
    assert!(panel
        .tree()
        .nodes
        .iter()
        .any(|(_, node)| node.value() == Some(&channels("8"))));

    let default_label = tr("系统默认设备（{0}）").replace("{0}", "Speakers");
    let Some(HostAction::Audio(AudioAction::OutputDevice(device))) =
        panel.select("输出设备", &default_label)
    else {
        panic!("Expected to select the system default");
    };
    assert_eq!(device, None);
    panel.prefs.audio.output_device = device;
    assert!(panel
        .tree()
        .nodes
        .iter()
        .any(|(_, node)| node.value() == Some(&channels("2"))));
}

#[test]
fn default_output_can_use_surround_without_listed_channel_capabilities() {
    for default_listed in [false, true] {
        let mut panel = Panel::new();
        let mut entries = panel.settings.devices.devices.take().unwrap().entries;
        entries[0].is_default = default_listed;
        entries[0].max_channels = None;
        panel.settings.devices.devices = Some(DeviceList::inspect(entries, || Ok(6)));
        panel.select("输出声道", tr(Layout::Surround51.name()));
        assert_eq!(panel.settings.routing.unwrap().layout, Layout::Surround51);
        panel.prefs.audio.output_device = Some("a".into());
        assert!(panel
            .tree()
            .nodes
            .iter()
            .any(|(_, node)| node.value() == Some(tr("未检测到"))));
    }
}
