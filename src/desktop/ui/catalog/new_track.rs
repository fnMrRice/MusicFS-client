use super::{style, Services};
use crate::{i18n::tr, task::Task};
use eframe::egui;
use serde_json::json;

#[derive(Default)]
pub(super) struct NewTrack {
    title: String,
    artists: String,
    request: Option<Task<()>>,
    key: Option<String>,
    error: Option<String>,
}

impl NewTrack {
    // 完成或取消时关闭窗口。提交结果未知时保持原始输入与幂等键。
    pub fn show(&mut self, ui: &egui::Ui, svc: Services<'_>) -> Option<bool> {
        if let Some(result) = self.request.as_mut().and_then(Task::poll) {
            self.request = None;
            match result {
                Ok(()) => return Some(true),
                Err(error) => {
                    if error.contains("HTTP 4") {
                        self.key = None;
                    }
                    self.error = Some(error);
                }
            }
        }
        let busy = self.request.is_some();
        let mut save = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("new-music-track")).show(ui.ctx(), |ui| {
            ui.set_min_width(260.0);
            ui.heading(tr("新建曲目"));
            ui.add_space(12.);
            ui.add_enabled_ui(!busy && self.key.is_none(), |ui| {
                let title = ui.label(tr("曲名"));
                ui.text_edit_singleline(&mut self.title)
                    .labelled_by(title.id);
                let artists = ui.label(tr("歌手（每行一个，可选）"));
                ui.add(egui::TextEdit::multiline(&mut self.artists).desired_rows(3))
                    .labelled_by(artists.id);
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
                if self.key.is_some() {
                    ui.weak(tr("等待确认上一次提交，请使用相同内容重试。"));
                }
            }
            ui.horizontal(|ui| {
                save = style::primary(
                    ui,
                    tr(if self.key.is_some() {
                        "重试"
                    } else {
                        "保存"
                    }),
                    !busy && !self.title.trim().is_empty(),
                )
                .clicked();
                cancel = style::add_button(ui, !busy, egui::Button::new(tr("取消"))).clicked();
                if busy {
                    ui.spinner();
                }
            });
        });
        if !busy && (modal.should_close() || cancel) {
            return Some(false);
        }
        if save {
            let key = self
                .key
                .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
                .clone();
            let server = svc.server.clone();
            let body = json!({"title": self.title.trim(), "artistNames": self.artists.lines().map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>()});
            self.error = None;
            self.request = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                let result = server
                    .settings_json(
                        "POST",
                        "/library/tracks",
                        Some(&body),
                        &[("Idempotency-Key", &key)],
                    )
                    .await?;
                result["trackId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|_| ())
                    .ok_or("Invalid MusicFS track response.".into())
            }));
        }
        None
    }
}
