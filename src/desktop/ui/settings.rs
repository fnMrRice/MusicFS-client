use super::{themes, HostAction};
use crate::model::Preferences;
use eframe::egui;
use musicfs_server_adapter::SessionInfo;

pub(super) fn show(
    ui: &mut egui::Ui,
    info: &SessionInfo,
    prefs: &Preferences,
    busy: bool,
    action: &mut Option<HostAction>,
) {
    ui.heading("连接与账号");
    ui.add_space(8.0);
    egui::Grid::new("identity")
        .min_col_width(100.0)
        .spacing([20.0, 16.0])
        .show(ui, |ui| {
            for (label, value) in [
                ("当前账号", &info.user_name),
                ("服务器", &info.endpoint),
                ("服务器版本", &info.server_version),
            ] {
                ui.weak(label);
                ui.label(value);
                ui.end_row();
            }
        });
    ui.add_space(18.0);
    if ui
        .add_enabled(!busy, egui::Button::new("退出登录"))
        .clicked()
    {
        *action = Some(HostAction::Logout);
    }
    ui.add_space(15.0);
    ui.separator();
    ui.heading("外观");
    ui.horizontal_wrapped(|ui| {
        for (value, name) in themes() {
            if ui.selectable_label(prefs.theme == value, name).clicked() {
                *action = Some(HostAction::Theme(value.into()));
            }
        }
    });
    ui.add_space(15.0);
    ui.separator();
    ui.heading("播放");
    let mut fades = prefs.fades;
    ui.checkbox(&mut fades.seek, "跳转时淡出淡入");
    ui.checkbox(&mut fades.track_change, "切歌时淡出淡入");
    if fades != prefs.fades {
        *action = Some(HostAction::Fades(fades));
    }
}
