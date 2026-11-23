use crate::i18n::tr;
mod accounts;
mod album;
mod anonymous;
mod api;
mod directories;
mod local;
mod scans;
mod watch;

use super::Services;
use api::{list, name, text, Load};
use eframe::egui;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct Sources {
    providers: Load<Vec<Value>>,
    online: Load<Vec<Value>>,
    plugins: Load<Vec<Value>>,
    locals: HashMap<String, local::Local>,
    accounts: HashMap<String, accounts::Account>,
    searches: HashMap<String, anonymous::Search>,
    album: album::Order,
    directory_request: Option<String>,
}

impl Sources {
    pub(super) fn open_directory(&mut self, id: String) {
        self.directory_request = Some(id);
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        let admin = svc.server.info.role == "admin";
        let refresh = ui.button(tr("刷新音乐源")).clicked();
        for (key, path, load) in [
            ("providers", "/provider-instances", &mut self.providers),
            ("online", "/online-instances", &mut self.online),
            ("plugins", "/admin/plugins", &mut self.plugins),
        ] {
            if !admin && key == "plugins" {
                continue;
            }
            load.poll();
            let retry = ui.push_id(key, |ui| load.status(ui)).inner;
            if load.initial() || retry || refresh {
                let server = svc.server.clone();
                load.start(ui, svc, async move { list(&server, path).await });
            }
        }
        let online = self.online.value.as_deref().unwrap_or_default();
        let providers = self.providers.value.as_deref().unwrap_or_default();
        let mut visible = HashSet::new();
        for instance in online.iter().filter(|v| is_source(v)) {
            let id = text(instance, "instanceId");
            visible.insert(id);
            ui.push_id(("online", id), |ui| {
                ui.heading(name(instance));
                if text(instance, "authentication") == "anonymous" {
                    ui.weak(tr("匿名访问公开内容，无需平台登录。"));
                    self.searches
                        .entry(id.into())
                        .or_default()
                        .show(ui, svc, instance);
                } else {
                    self.accounts
                        .entry(id.into())
                        .or_default()
                        .show(ui, svc, instance, refresh);
                }
                ui.separator();
            });
        }
        let plugins = self.plugins.value.as_deref().unwrap_or_default();
        for plugin in plugins
            .iter()
            .filter(|p| admin && text(p, "pluginId") == "musicfs.plugin.source.localfiles")
        {
            for instance in plugin["instances"].as_array().into_iter().flatten() {
                let id = text(instance, "instanceId");
                if !visible.insert(id) {
                    continue;
                }
                let open = self.directory_request.as_deref() == Some(id);
                if open {
                    self.directory_request = None;
                }
                let provider = providers.iter().find(|p| text(p, "id") == id);
                ui.push_id(("local", id), |ui| {
                    let title = ui.heading(name(plugin));
                    if open {
                        title.scroll_to_me(Some(egui::Align::Min));
                    }
                    self.locals
                        .entry(id.into())
                        .or_default()
                        .show(ui, svc, instance, provider, open, refresh);
                    ui.separator();
                });
            }
        }
        for provider in providers.iter().filter(|p| is_source(p)) {
            if visible.insert(text(provider, "id")) {
                ui.heading(name(provider));
                ui.weak(
                    if matches!(text(provider, "availability"), "available" | "online") {
                        tr("可用")
                    } else {
                        tr("暂不可用")
                    },
                );
                ui.separator();
            }
        }
        if visible.is_empty() && !self.providers.busy() && !self.online.busy() {
            ui.weak(tr("暂无音乐来源"));
        }
        self.album.show(ui, svc);
    }
}
fn is_source(v: &Value) -> bool {
    v["plugin"].is_null()
        || v.pointer("/plugin/features")
            .and_then(Value::as_array)
            .is_some_and(|features| features.iter().any(|f| f == "music-source"))
}
