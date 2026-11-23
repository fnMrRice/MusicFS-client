use super::{
    api::{decode, get, list, text, Load},
    Services,
};
use crate::i18n::tr;
use eframe::egui;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

const PATH: &str = "/scrapers/album-auto-settings";
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Settings {
    revision: u64,
    local_plugin_order: Vec<String>,
}
#[derive(Default)]
pub(super) struct Order {
    settings: Load<(Settings, Vec<Value>)>,
    save: Load<Settings>,
    draft: Vec<String>,
    key: Option<String>,
    saved: bool,
}
impl Order {
    pub fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        ui.heading(tr("本地专辑刮削顺序"));
        ui.weak(tr(
            "按以下顺序查找本地专辑。网易云音乐和 QQ 音乐的专辑优先使用各自的平台。",
        ));
        if self.settings.poll() {
            self.draft = self
                .settings
                .value
                .as_ref()
                .unwrap()
                .0
                .local_plugin_order
                .clone();
            self.key = None;
        }
        if self.save.poll() {
            let saved = self.save.value.take().unwrap();
            self.draft = saved.local_plugin_order.clone();
            if let Some((settings, _)) = &mut self.settings.value {
                *settings = saved;
            }
            self.key = None;
            self.saved = true;
        }
        let retry = self.settings.status(ui);
        let conflict = self
            .save
            .error
            .as_ref()
            .is_some_and(|e| e.contains("HTTP 409") || e.contains("HTTP 412"));
        if self.settings.initial() || retry || conflict && ui.button(tr("重新加载设置")).clicked()
        {
            self.save.error = None;
            let s = svc.server.clone();
            self.settings.start(ui, svc, async move {
                Ok((get(&s, PATH).await?, list(&s, "/scrapers").await?))
            });
        }
        let Some((settings, scrapers)) = &self.settings.value else {
            return;
        };
        let name = |id: &str| {
            scrapers
                .iter()
                .find(|v| text(v, "pluginId") == id)
                .map_or_else(|| id.to_owned(), |v| text(v, "name").to_owned())
        };
        ui.add_enabled_ui(
            !self.save.busy() && !self.settings.busy() && !conflict,
            |ui| {
                let mut move_to = None;
                let mut remove = None;
                for (i, id) in self.draft.iter().enumerate() {
                    ui.push_id(id, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(name(id));
                            if ui.add_enabled(i > 0, egui::Button::new("↑")).clicked() {
                                move_to = Some((i, i - 1));
                            }
                            if ui
                                .add_enabled(i + 1 < self.draft.len(), egui::Button::new("↓"))
                                .clicked()
                            {
                                move_to = Some((i, i + 1));
                            }
                            if ui
                                .add_enabled(self.draft.len() > 1, egui::Button::new(tr("移除")))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        })
                    });
                }
                let mut changed = false;
                if let Some((a, b)) = move_to {
                    self.draft.swap(a, b);
                    changed = true;
                }
                if let Some(i) = remove {
                    self.draft.remove(i);
                    changed = true;
                }
                egui::ComboBox::from_id_salt("scraper-add")
                    .selected_text(tr("添加刮削来源"))
                    .show_ui(ui, |ui| {
                        for v in scrapers {
                            let id = text(v, "pluginId");
                            if v["available"] == true
                                && v["requiresExternalId"] != true
                                && ["albumDetails", "albumTracks"].iter().all(|f| {
                                    v["fields"]
                                        .as_array()
                                        .is_some_and(|a| a.iter().any(|x| x == *f))
                                })
                                && !self.draft.iter().any(|p| p == id)
                                && ui.button(name(id)).clicked()
                            {
                                self.draft.push(id.into());
                                changed = true;
                                ui.close_menu();
                            }
                        }
                    });
                if changed {
                    self.key = None;
                    self.save.error = None;
                    self.saved = false;
                }
                if ui
                    .add_enabled(
                        self.draft != settings.local_plugin_order,
                        egui::Button::new(tr("保存设置")),
                    )
                    .clicked()
                {
                    let key = self
                        .key
                        .get_or_insert_with(|| Uuid::new_v4().to_string())
                        .clone();
                    let body =
                        json!({"expectedRevision":settings.revision,"localPluginOrder":self.draft});
                    let s = svc.server.clone();
                    self.save.start(ui, svc, async move {
                        decode(
                            s.settings_json("PUT", PATH, Some(&body), &[("Idempotency-Key", &key)])
                                .await?,
                        )
                    });
                }
            },
        );
        if let Some(e) = &self.save.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if self.saved {
            ui.weak(tr("设置已保存。"));
        }
    }
}
