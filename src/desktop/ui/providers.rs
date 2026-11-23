use super::Services;
use crate::{
    i18n::{language_tags, tr},
    task::Task,
};
use eframe::egui;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Identity {
    name: String,
    #[serde(default, rename = "localizedNames")]
    names: HashMap<String, String>,
}
#[derive(Deserialize)]
struct Root {
    id: String,
    name: String,
}
#[derive(Deserialize)]
struct Provider {
    id: String,
    name: String,
    plugin: Option<Identity>,
    #[serde(default)]
    roots: Option<Vec<Root>>,
}
impl Provider {
    fn label(&self) -> &str {
        let Some(plugin) = &self.plugin else {
            return if self.name.is_empty() || self.name == self.id {
                tr("未知来源")
            } else {
                &self.name
            };
        };
        language_tags()
            .iter()
            .find_map(|&key| plugin.names.get(key).map(String::as_str))
            .unwrap_or(&plugin.name)
    }
}
#[derive(Default)]
pub struct Providers {
    pub selected: Option<String>,
    items: Vec<Provider>,
    task: Option<Task<Vec<Provider>>>,
    started: bool,
    error: Option<String>,
}
impl Providers {
    pub fn root_label(&self, provider: &str, root: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|p| p.id == provider)?
            .roots
            .as_ref()?
            .iter()
            .find(|r| r.id == root)
            .map(|r| r.name.as_str())
    }
    pub fn sync(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        if let Some(result) = self.task.as_mut().and_then(Task::poll) {
            self.task = None;
            match result {
                Ok(items) => self.items = items,
                Err(e) => self.error = Some(e),
            }
        }
        if !self.started {
            self.started = true;
            let server = svc.server.clone();
            self.task = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                #[derive(Deserialize)]
                struct Page {
                    items: Vec<Provider>,
                }
                let value = server
                    .settings_json("GET", "/provider-instances", None, &[])
                    .await?;
                serde_json::from_value::<Page>(value)
                    .map(|p| p.items)
                    .map_err(|_| "Invalid provider list.".into())
            }));
        }
    }
    pub fn label(&self, id: &str) -> &str {
        self.items
            .iter()
            .find(|p| p.id == id)
            .map_or(tr("未知来源"), Provider::label)
    }
    pub fn select(&mut self, ui: &mut egui::Ui) -> bool {
        let old = self.selected.clone();
        egui::ComboBox::from_label(tr("来源"))
            .selected_text(
                self.selected
                    .as_ref()
                    .map(|id| self.label(id))
                    .unwrap_or(tr("全部来源")),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.selected, None, tr("全部来源"));
                for item in &self.items {
                    ui.selectable_value(&mut self.selected, Some(item.id.clone()), item.label());
                }
            });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button(tr("重试")).clicked() {
                self.started = false;
                self.error = None;
            }
        }
        old != self.selected
    }
}
