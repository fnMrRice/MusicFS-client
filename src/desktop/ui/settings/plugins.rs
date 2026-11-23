mod catalog;
mod editor;
mod entry;
mod schema;
mod switches;
#[cfg(test)]
mod tests;

use super::Services;
use crate::{i18n::tr, task::Task};
use catalog::{purpose, Online, Snapshot};
use editor::Editor;
use eframe::egui;
use musicfs_server_adapter::ServerSession;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Plugins {
    list: Resource<Snapshot>,
    online: Resource<Online>,
    all_users: bool,
    search: String,
    purposes: BTreeSet<String>,
    expanded: BTreeSet<String>,
    directory: Option<String>,
    editor: Option<Editor>,
    switches: switches::Switches,
}

impl Plugins {
    pub(super) fn take_directory_request(&mut self) -> Option<String> {
        self.directory.take()
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        if svc.server.info.role != "admin" {
            ui.weak(tr("需要管理员权限。"));
            return;
        }
        let ctx = ui.ctx().clone();
        let path = if self.all_users {
            "/admin/plugins?allUsers=true"
        } else {
            "/admin/plugins"
        };
        self.list.update(&ctx, svc, path);
        self.online.update(&ctx, svc, "/online-instances");
        let mut refresh = self.switches.show(ui, svc);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search).hint_text(tr("按 ID 或名称筛选插件")),
            );
            if ui
                .add_enabled(
                    !self.list.busy() && !self.online.busy(),
                    egui::Button::new(tr("刷新")),
                )
                .on_hover_text(tr("刷新插件列表"))
                .clicked()
            {
                refresh = true;
            }
        });
        let options: BTreeSet<String> = self
            .list
            .data
            .iter()
            .flat_map(|s| s.items.iter())
            .flat_map(|p| p.purposes())
            .map(str::to_string)
            .chain(self.purposes.iter().cloned())
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.strong(tr("插件类型")).on_hover_text(tr(
                "可多选；按住 Shift 点击可仅选择该类型；未选择时显示全部。",
            ));
            if ui
                .selectable_label(self.purposes.is_empty(), tr("全部"))
                .clicked()
            {
                self.purposes.clear();
            }
            for item in options {
                let mut selected = self.purposes.contains(&item);
                if ui.checkbox(&mut selected, tr(purpose(&item))).clicked() {
                    if ui.input(|i| i.modifiers.shift) {
                        self.purposes.clear();
                        selected = true;
                    }
                    if selected {
                        self.purposes.insert(item);
                    } else {
                        self.purposes.remove(&item);
                    }
                }
            }
        });
        if ui
            .checkbox(&mut self.all_users, tr("显示所有用户实例"))
            .changed()
        {
            self.list = Resource::default();
            ctx.request_repaint();
        } else if refresh {
            self.list.start(&ctx, svc, path);
            self.online.start(&ctx, svc, "/online-instances");
        }
        if self.list.report(ui) {
            self.list.start(&ctx, svc, path);
        }
        if self.online.report(ui) {
            self.online.start(&ctx, svc, "/online-instances");
        }
        if self.list.busy() {
            ui.weak(tr("正在读取插件…"));
        }
        if let Some(snapshot) = &self.list.data {
            if snapshot.restart_required {
                ui.label(tr("配置变更需要重启服务端后生效。"));
            }
            let query = self.search.trim().to_lowercase();
            let mut visible = false;
            for p in snapshot
                .items
                .iter()
                .filter(|p| p.matches(&self.purposes, &query))
            {
                visible = true;
                let mut expanded = self.expanded.contains(&p.plugin_id);
                let writable = snapshot.writable && !self.list.busy() && !self.switches.busy();
                let command = ui
                    .push_id(&p.plugin_id, |ui| {
                        entry::show(ui, p, self.online.data.as_ref(), &mut expanded, writable)
                    })
                    .inner;
                if expanded {
                    self.expanded.insert(p.plugin_id.clone());
                } else {
                    self.expanded.remove(&p.plugin_id);
                }
                match command {
                    Some(entry::Command::Configure) => {
                        self.editor = Some(Editor::new(&p.plugin_id, p.name()))
                    }
                    Some(entry::Command::Directory(id)) => self.directory = Some(id),
                    Some(entry::Command::Switch { instance, enabled }) => {
                        self.switches.start(
                            &ctx,
                            svc,
                            &p.plugin_id,
                            instance.as_deref(),
                            enabled,
                            &snapshot.revision,
                        );
                    }
                    None => {}
                }
            }
            if !visible {
                ui.weak(tr(if snapshot.items.is_empty() {
                    "未发现已安装插件。"
                } else {
                    "没有符合筛选条件的插件。"
                }));
            }
        }
        if self.editor.as_mut().is_some_and(|e| e.show(&ctx, svc)) {
            self.editor = None;
        }
    }
}

struct Resource<T> {
    data: Option<T>,
    pending: Option<Task<T>>,
    error: Option<String>,
}

impl<T> Default for Resource<T> {
    fn default() -> Self {
        Self {
            data: None,
            pending: None,
            error: None,
        }
    }
}

impl<T: DeserializeOwned + Send + 'static> Resource<T> {
    fn start(&mut self, ctx: &egui::Context, svc: Services<'_>, path: &str) {
        let server = svc.server.clone();
        let path = path.to_owned();
        self.error = None;
        self.pending = Some(Task::spawn(svc.runtime, ctx, async move {
            request(&server, "GET", &path, None, &[]).await
        }));
    }

    fn update(&mut self, ctx: &egui::Context, svc: Services<'_>, path: &str) {
        if self.data.is_none() && self.error.is_none() && !self.busy() {
            self.start(ctx, svc, path);
        }
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(v) => {
                    self.data = Some(v);
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            }
        }
    }

    fn report(&self, ui: &mut egui::Ui) -> bool {
        self.error.as_ref().is_some_and(|e| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, e);
                ui.button(tr("重试")).clicked()
            })
            .inner
        })
    }

    fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

async fn request<T: DeserializeOwned>(
    server: &ServerSession,
    method: &str,
    path: &str,
    body: Option<&Value>,
    headers: &[(&str, &str)],
) -> Result<T, String> {
    let result = server.settings_json(method, path, body, headers).await?;
    serde_json::from_value(result).map_err(|_| "Invalid MusicFS plugin response.".to_owned())
}

fn segment(id: &str) -> Result<&str, String> {
    if id.is_empty()
        || matches!(id, "." | "..")
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err("Invalid MusicFS plugin identifier.".into());
    }
    Ok(id)
}

fn uncertain(error: &str) -> bool {
    !error.contains("(HTTP 4")
}
