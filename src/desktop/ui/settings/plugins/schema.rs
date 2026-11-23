use super::catalog::{localized, Translations};
use crate::i18n::tr;
use eframe::egui;
use serde::Deserialize;
use serde_json::{Number, Value};
use std::collections::BTreeMap;

pub(super) type Values = BTreeMap<String, Value>;
pub(super) type Draft = BTreeMap<String, String>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Settings {
    pub plugin_id: String,
    pub revision: u64,
    pub schema: Schema,
    pub values: Values,
    pub job: Option<Job>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Schema {
    pub revision: String,
    pub title: String,
    #[serde(default)]
    pub localized_titles: Translations,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub localized_descriptions: Translations,
    pub fields: Vec<Field>,
    pub actions: Vec<Action>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Field {
    pub key: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(flatten)]
    text: FieldText,
    default_value: Value,
    #[serde(default)]
    required: bool,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
    max_length: Option<usize>,
    #[serde(default)]
    options: Vec<Choice>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FieldText {
    label: String,
    #[serde(default)]
    localized_labels: Translations,
    #[serde(default)]
    description: String,
    #[serde(default)]
    localized_descriptions: Translations,
}

impl FieldText {
    pub fn label(&self) -> &str {
        localized(&self.label, &self.localized_labels)
    }
    pub fn help(&self, ui: &mut egui::Ui) {
        let description = localized(&self.description, &self.localized_descriptions);
        if !description.is_empty() {
            ui.weak(description);
        }
    }
}

#[derive(Deserialize)]
struct Choice {
    value: String,
    #[serde(flatten)]
    text: FieldText,
}

#[derive(Deserialize)]
pub(super) struct Action {
    pub id: String,
    #[serde(flatten)]
    pub text: FieldText,
}

impl Settings {
    pub fn draft(&self) -> Draft {
        self.schema
            .fields
            .iter()
            .map(|f| {
                let value = self
                    .values
                    .get(&f.key)
                    .filter(|v| !v.is_null())
                    .unwrap_or(&f.default_value);
                let value = match value {
                    Value::String(s) => s.to_owned(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(v) => v.to_string(),
                    _ => String::new(),
                };
                (f.key.clone(), value)
            })
            .collect()
    }
}

impl Schema {
    pub fn values(&self, draft: &Draft) -> Option<Values> {
        self.fields
            .iter()
            .map(|f| Some((f.key.clone(), f.value(draft.get(&f.key)?)?)))
            .collect()
    }
}

impl Field {
    pub fn show(&self, ui: &mut egui::Ui, draft: &mut Draft) {
        let value = draft.entry(self.key.clone()).or_default();
        let label = self.text.label();
        if self.kind == "checkbox" {
            let mut checked = value == "true";
            if ui.checkbox(&mut checked, label).changed() {
                *value = checked.to_string();
            }
        } else {
            let description = ui.label(label);
            match self.kind.as_str() {
                "text" | "number" => {
                    let response = ui.add(egui::TextEdit::singleline(value).desired_width(300.0));
                    response.labelled_by(description.id);
                }
                "select" => {
                    let selected = self
                        .options
                        .iter()
                        .find(|o| o.value == *value)
                        .map_or("", |o| o.text.label());
                    egui::ComboBox::from_id_salt(&self.key)
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for o in &self.options {
                                ui.selectable_value(value, o.value.clone(), o.text.label());
                            }
                        })
                        .response
                        .labelled_by(description.id);
                }
                _ => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        tr("此设置控件暂不支持，请更新桌面客户端。"),
                    );
                }
            }
        }
        self.text.help(ui);
    }

    fn value(&self, raw: &str) -> Option<Value> {
        match self.kind.as_str() {
            "checkbox" => raw.parse().ok().map(Value::Bool),
            "text"
                if (!self.required || !raw.trim().is_empty())
                    && self
                        .max_length
                        .is_none_or(|m| raw.encode_utf16().count() <= m) =>
            {
                Some(Value::String(raw.into()))
            }
            "select" if self.options.iter().any(|o| o.value == raw) => {
                Some(Value::String(raw.into()))
            }
            "number" => {
                let number: f64 = raw.trim().parse().ok()?;
                if !number.is_finite()
                    || self.min.is_some_and(|min| number < min)
                    || self.max.is_some_and(|max| number > max)
                {
                    return None;
                }
                if let Some(step) = self.step {
                    let ticks = (number - self.min.unwrap_or_default()) / step;
                    if !ticks.is_finite() || step <= 0.0 || (ticks - ticks.round()).abs() >= 1e-8 {
                        return None;
                    }
                }
                // 整数字段保持整数形式，避免服务端把 JSON 小数拒为非法整数。
                if let Ok(n) = raw.trim().parse::<i64>() {
                    return Some(Value::from(n));
                }
                Number::from_f64(number).map(Value::Number)
            }
            _ => None,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Job {
    pub status: String,
    processed: u64,
    total: u64,
    candidates: Option<u64>,
}

impl Job {
    pub fn active(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running")
    }
    pub fn show(&self, ui: &mut egui::Ui) {
        let title = match self.status.as_str() {
            "queued" => "任务等待执行",
            "running" => "任务正在执行",
            "succeeded" => "任务已完成",
            "failed" => "任务失败，可重新执行",
            _ => return,
        };
        ui.strong(tr(title));
        let mut summary = tr("已处理 {0} / {1} 首曲目")
            .replace("{0}", &self.processed.to_string())
            .replace("{1}", &self.total.to_string());
        if let Some(n) = self.candidates {
            summary.push_str(" · ");
            summary.push_str(&tr("发现 {0} 对候选").replace("{0}", &n.to_string()));
        }
        ui.label(summary);
        if self.status == "running" && self.total > 0 {
            ui.add(egui::ProgressBar::new(
                (self.processed as f64 / self.total as f64).clamp(0.0, 1.0) as f32,
            ));
        }
        if self.status == "failed" {
            ui.weak(tr("请检查插件是否启用，以及音乐源是否可用，然后重试。"));
        }
    }
}
