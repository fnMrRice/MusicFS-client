use super::{
    api::Load,
    directories::{apply, Config},
    Services,
};
use crate::i18n::tr;
use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Policy {
    enabled: bool,
    debounce_ms: u32,
    max_delay_ms: u32,
    full_rescan_seconds: u32,
}

#[derive(Default)]
pub(super) struct Watch {
    draft: Option<(String, Policy)>,
    save: Load<String>,
    key: Option<String>,
}

impl Watch {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        id: &str,
        config: &Config,
    ) -> bool {
        let Some(saved) = &config.watch else {
            return false;
        };
        if self.save.poll() {
            self.draft = None;
            self.key = None;
            return true;
        }
        let (revision, draft) = self
            .draft
            .get_or_insert_with(|| (config.revision.clone(), saved.clone()));
        if revision != &config.revision {
            revision.clone_from(&config.revision);
            draft.clone_from(saved);
            self.key = None;
            self.save.error = None;
        }
        let mut reload = false;
        ui.collapsing(tr("目录变更监听"), |ui| {
            let mut changed = false;
            let conflict = self.save.error.as_ref().is_some_and(|e| {
                e.contains("HTTP 409") || e.contains("HTTP 412") || e.contains("superseded")
            });
            ui.add_enabled_ui(!self.save.busy() && !conflict, |ui| {
                changed |= ui
                    .checkbox(&mut draft.enabled, tr("监听音乐目录变更"))
                    .changed();
                ui.add_enabled_ui(draft.enabled, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr("变更合并等待时间（毫秒）"));
                        changed |= ui
                            .add(egui::DragValue::new(&mut draft.debounce_ms).range(100..=30000))
                            .changed();
                    });
                    draft.max_delay_ms = draft.max_delay_ms.max(draft.debounce_ms);
                    ui.horizontal(|ui| {
                        ui.label(tr("最长延迟（毫秒）"));
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut draft.max_delay_ms)
                                    .range(draft.debounce_ms..=60000),
                            )
                            .changed();
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("全量重扫间隔（秒）"));
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut draft.full_rescan_seconds)
                                    .range(30..=86400),
                            )
                            .changed();
                    });
                });
                if changed {
                    self.key = None;
                    self.save.error = None;
                }
                if ui
                    .add_enabled(draft != saved, egui::Button::new(tr("保存监听设置")))
                    .clicked()
                {
                    let key = self
                        .key
                        .get_or_insert_with(|| Uuid::new_v4().to_string())
                        .clone();
                    let (s, instance, revision, body) = (
                        svc.server.clone(),
                        id.to_owned(),
                        revision.clone(),
                        json!({"watch":draft}),
                    );
                    let activity = svc.activity.subscribe();
                    self.save.start(ui, svc, async move {
                        apply(&s, activity, &instance, &revision, &key, body).await
                    });
                }
            });
            if let Some(e) = &self.save.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            if conflict && ui.button(tr("重新加载设置")).clicked() {
                reload = true;
            }
            if self.save.busy() {
                ui.spinner();
            }
        });
        if reload {
            self.draft = None;
            self.key = None;
            self.save.error = None;
        }
        reload
    }
}
