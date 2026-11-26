mod audio;
mod plugins;
mod sources;

use super::{data::Services, themes, HostAction};
use crate::{i18n::tr, model::Preferences};
use eframe::egui;

const SECTIONS: [&str; 7] = [
    "音乐源",
    "插件管理",
    "播放配置",
    "音效与输出",
    "外观与语言",
    "用户",
    "关于",
];

#[derive(Default)]
pub(super) struct Settings {
    tab: usize,
    audio: audio::AudioSettingsUi,
    plugins: plugins::Plugins,
    sources: sources::Sources,
}

pub(super) fn language_menu(ui: &mut egui::Ui, current: &str) -> Option<String> {
    let options = [
        ("system", tr("跟随系统")),
        ("en", "English"),
        ("zh-CN", "简体中文"),
        ("zh-TW", "繁體中文"),
        ("ja", "日本語"),
    ];
    let mut selected = current;
    egui::ComboBox::from_label(tr("语言"))
        .selected_text(
            options
                .iter()
                .find(|(id, _)| *id == current)
                .unwrap_or(&options[0])
                .1,
        )
        .show_ui(ui, |ui| {
            for (value, label) in options {
                ui.selectable_value(&mut selected, value, label);
            }
        });
    (selected != current).then(|| selected.to_owned())
}

impl Settings {
    pub(super) fn open_plugins(&mut self) {
        self.tab = 1;
    }
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        prefs: &Preferences,
        busy: bool,
        action: &mut Option<HostAction>,
    ) {
        ui.horizontal_wrapped(|ui| {
            for (i, label) in SECTIONS.iter().enumerate() {
                ui.selectable_value(&mut self.tab, i, tr(label));
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("settings", self.tab))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading(tr(SECTIONS[self.tab]));
                ui.add_space(10.);
                let info = &svc.server.info;
                match self.tab {
                    0 => self.sources.show(ui, svc),
                    1 if info.role == "admin" => {
                        self.plugins.show(ui, svc);
                        if let Some(id) = self.plugins.take_directory_request() {
                            self.sources.open_directory(id);
                            self.tab = 0;
                        }
                    }
                    1 => {
                        ui.weak(tr("插件管理仅对管理员开放。"));
                    }
                    2 => self.audio.playback(ui, svc, prefs, action),
                    3 => self
                        .audio
                        .local_dsp(ui, svc.player, svc.runtime, prefs, action),
                    4 => {
                        ui.horizontal_wrapped(|ui| {
                            for (value, label) in themes() {
                                if ui
                                    .selectable_label(prefs.theme == value, tr(label))
                                    .clicked()
                                {
                                    *action = Some(HostAction::Theme(value.into()));
                                }
                            }
                        });
                        if let Some(value) = super::style::accent_choices(ui, &prefs.accent) {
                            *action = Some(HostAction::Accent(value));
                        }
                        ui.add_space(10.);
                        if let Some(value) = language_menu(ui, &prefs.language) {
                            *action = Some(HostAction::Language(value));
                        }
                    }
                    5 => {
                        ui.label(format!(
                            "{}   {} · {}",
                            tr("当前账号"),
                            info.user_name,
                            info.role
                        ));
                        if ui
                            .add_enabled(!busy, egui::Button::new(tr("退出登录")))
                            .clicked()
                        {
                            *action = Some(HostAction::Logout);
                        }
                    }
                    _ => {
                        egui::Grid::new("about").spacing([20., 12.]).show(ui, |ui| {
                            for (label, value) in [
                                ("音乐服务", info.endpoint.as_str()),
                                ("服务端版本", &info.server_version),
                                ("桌面端版本", env!("CARGO_PKG_VERSION")),
                                ("实例", &info.instance_id),
                                ("协议", "MusicFS API v1"),
                            ] {
                                ui.weak(tr(label));
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                    }
                }
            });
    }
}
