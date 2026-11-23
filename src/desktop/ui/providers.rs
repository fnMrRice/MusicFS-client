use super::Services;
use crate::{
    i18n::{language_tags, tr},
    task::Task,
};
use eframe::egui;
use serde::Deserialize;
use std::collections::HashMap;

pub(super) fn dropdown(id: &'static str, width: f32) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .truncate()
        .icon(|ui, rect, visuals, open, _| {
            let center = rect.center();
            let dy = if open { -2.5 } else { 2.5 };
            ui.painter().add(egui::Shape::line(
                vec![
                    center + egui::vec2(-4.5, -dy),
                    center + egui::vec2(0., dy),
                    center + egui::vec2(4.5, -dy),
                ],
                egui::Stroke::new(1.4_f32, visuals.fg_stroke.color),
            ));
        })
}

// 保留不可操作的选择框语义，并让值与普通表单文字保持同样的可读性。
pub(super) fn disabled_select(
    ui: &mut egui::Ui,
    id: &'static str,
    label: &str,
    width: f32,
    font_size: f32,
    help: &str,
) {
    let painter = ui.painter().clone();
    let color = ui.visuals().text_color();
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.disable();
            let response = dropdown(id, width)
                .selected_text("")
                .show_ui(ui, |_| {})
                .response;
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, false, label)
            });
            let available = egui::Rect::from_min_max(
                response.rect.left_top() + egui::vec2(12., 0.),
                response.rect.right_bottom() - egui::vec2(22., 0.),
            );
            painter
                .with_clip_rect(available.intersect(painter.clip_rect()))
                .text(
                    available.left_center(),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(font_size),
                    color,
                );
            response.on_disabled_hover_text(help);
        },
    );
}
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
        let response = dropdown("library-provider", 125.)
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
            })
            .response;
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, tr("来源"))
        });
        response.on_hover_text(tr("来源"));
        old != self.selected
    }
    pub fn refresh(&mut self) {
        self.started = false;
        self.error = None;
    }
    pub fn status(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button(tr("重试")).clicked() {
                self.started = false;
                self.error = None;
            }
        }
    }
}
