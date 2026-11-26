use super::{interaction, with_motion};
use eframe::egui::{self, collapsing_header::CollapsingResponse, Id, InnerResponse, Ui};

// 复合控件自行生成点击区域的 ID，记录它以便在下一帧绘制前取得交互状态。
fn animated_trigger<R>(ui: &mut Ui, render: impl FnOnce(&mut Ui) -> (Id, R)) -> R {
    let slot = ui.next_auto_id().with("musicfs-choice-trigger");
    let previous = ui.data(|data| data.get_temp::<Id>(slot));
    let (id, result) = with_motion(ui, previous.unwrap_or(slot), ui.is_enabled(), render);
    if Some(id) != previous {
        ui.data_mut(|data| data.insert_temp(slot, id));
        if let Some(response) = ui.ctx().read_response(id) {
            interaction(ui, &response);
        }
    }
    result
}

pub trait ComboBoxMotion {
    fn show_animated<R>(
        self,
        ui: &mut Ui,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>>;
}

impl ComboBoxMotion for egui::ComboBox {
    fn show_animated<R>(
        self,
        ui: &mut Ui,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        animated_trigger(ui, |ui| {
            // egui 在单独的 Area 中绘制菜单，菜单项使用各自的交互状态。
            let result = self.show_ui(ui, contents);
            (result.response.id, result)
        })
    }
}

pub fn collapsing<R>(
    ui: &mut Ui,
    header: egui::CollapsingHeader,
    body: impl FnOnce(&mut Ui) -> R,
) -> CollapsingResponse<R> {
    let body_style = ui.style().clone();
    animated_trigger(ui, |ui| {
        let result = header.show_background(true).show(ui, |ui| {
            ui.set_style(body_style);
            body(ui)
        });
        (result.header_response.id, result)
    })
}
