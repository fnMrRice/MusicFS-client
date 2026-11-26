use super::Services;
use crate::i18n::{locale, tr};
use crate::task::Task;
use crate::ui::style;
use eframe::egui;
use musicfs_server_adapter::ServerSession;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::future::Future;

pub(super) struct Load<T> {
    pub value: Option<T>,
    pub error: Option<String>,
    pending: Option<Task<T>>,
}

impl<T> Default for Load<T> {
    fn default() -> Self {
        Self {
            value: None,
            error: None,
            pending: None,
        }
    }
}

impl<T: Send + 'static> Load<T> {
    pub fn start(
        &mut self,
        ui: &egui::Ui,
        svc: Services<'_>,
        f: impl Future<Output = Result<T, String>> + Send + 'static,
    ) {
        if self.pending.is_none() {
            self.error = None;
            self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), f));
        }
    }
    pub fn poll(&mut self) -> bool {
        let Some(result) = self.pending.as_mut().and_then(Task::poll) else {
            return false;
        };
        self.pending = None;
        match result {
            Ok(v) => {
                self.value = Some(v);
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }
    pub fn initial(&self) -> bool {
        self.pending.is_none() && self.value.is_none() && self.error.is_none()
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn status(&self, ui: &mut egui::Ui) -> bool {
        if self.busy() {
            ui.spinner();
        }
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
            return style::button(ui, tr("重试")).clicked();
        }
        false
    }
}

pub(super) async fn get<T: DeserializeOwned>(s: &ServerSession, path: &str) -> Result<T, String> {
    decode(s.settings_json("GET", path, None, &[]).await?)
}
pub(super) fn decode<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|_| "Invalid MusicFS settings response.".into())
}
pub(super) async fn list<T: DeserializeOwned>(
    s: &ServerSession,
    path: &str,
) -> Result<Vec<T>, String> {
    #[derive(Deserialize)]
    struct Items<T> {
        items: Vec<T>,
    }
    Ok(get::<Items<T>>(s, path).await?.items)
}
pub(super) fn path_id(id: &str) -> String {
    url::form_urlencoded::byte_serialize(id.as_bytes()).collect()
}
pub(super) fn name(v: &Value) -> &str {
    let tag = locale();
    let names = &v["plugin"]["localizedNames"];
    let script = match tag {
        "zh-TW" => "zh-Hant",
        "zh-CN" => "zh-Hans",
        _ => tag,
    };
    [tag, script, tag.split('-').next().unwrap_or(tag)]
        .into_iter()
        .find_map(|key| names[key].as_str().filter(|s| !s.is_empty()))
        .or_else(|| {
            ["/plugin/name", "/name", "/instanceId", "/id"]
                .into_iter()
                .find_map(|key| v.pointer(key)?.as_str().filter(|s| !s.is_empty()))
        })
        .unwrap_or_else(|| tr("未知来源"))
}
pub(super) fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
