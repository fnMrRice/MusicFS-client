use super::{popup, Services};
use crate::{i18n::tr, icons, task::Task};
use eframe::egui::{self, Ui};
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskPage {
    items: Vec<RefreshTask>,
    active_count: usize,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshTask {
    id: String,
    provider_name: String,
    owner_name: String,
    roots: Vec<Root>,
    status: String,
    cancellable: bool,
}

#[derive(Deserialize)]
struct Root {
    name: String,
}

#[derive(Default)]
pub(super) struct Queue {
    page: TaskPage,
    all: bool,
    more: bool,
    pending: Option<Task<TaskPage>>,
    cancel: Option<Task<()>>,
    error: Option<String>,
    checked: Option<Instant>,
}

impl Queue {
    fn load(&mut self, ui: &Ui, svc: Services<'_>, more: bool) {
        let scope = if self.all { "all" } else { "mine" };
        let mut q = url::form_urlencoded::Serializer::new(String::new());
        q.append_pair("scope", scope).append_pair("limit", "50");
        if more {
            let Some(cursor) = &self.page.next_cursor else {
                return;
            };
            q.append_pair("cursor", cursor);
        }
        let path = format!("/tasks?{}", q.finish());
        let server = svc.server.clone();
        self.more = more;
        self.checked = Some(Instant::now());
        self.error = None;
        self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            let data = server.settings_json("GET", &path, None, &[]).await?;
            serde_json::from_value(data).map_err(|_| "Invalid MusicFS task page.".into())
        }));
    }

    pub(super) fn button(&mut self, ui: &mut Ui, svc: Services<'_>) {
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(page) if self.more => {
                    self.page.items.extend(page.items);
                    self.page.next_cursor = page.next_cursor;
                    self.page.active_count = page.active_count;
                }
                Ok(page) => self.page = page,
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(result) = self.cancel.as_mut().and_then(Task::poll) {
            self.cancel = None;
            match result {
                Ok(()) => self.load(ui, svc, false),
                Err(e) => self.error = Some(e),
            }
        }
        let r = icons::button(ui, "queue", tr("任务队列"), 36., 22.);
        if self.pending.is_none()
            && (r.clicked()
                || self.checked.is_none_or(|t| {
                    t.elapsed()
                        > Duration::from_secs(if self.page.active_count > 0 { 5 } else { 60 })
                }) && !self.more)
        {
            self.load(ui, svc, false);
        }
        if self.page.active_count > 0 {
            let center = r.rect.right_top() + egui::vec2(-4., 6.);
            ui.painter()
                .circle_filled(center, 8., super::style::accent_fill(ui));
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                self.page.active_count.min(99),
                egui::FontId::proportional(10.),
                egui::Color32::WHITE,
            );
        }
        popup(ui, "tasks", &r, 315., |ui| {
            ui.heading(tr("任务队列"));
            if svc.server.info.role == "admin" {
                let before = self.all;
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.all, false, tr("我的任务"));
                    ui.selectable_value(&mut self.all, true, tr("所有任务"));
                });
                if self.all != before {
                    self.page = TaskPage::default();
                    self.load(ui, svc, false);
                }
            }
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
                if ui.button(tr("重试")).clicked() {
                    self.load(ui, svc, false);
                }
            } else if self.page.items.is_empty() {
                ui.weak(tr(if self.pending.is_some() {
                    "正在读取任务…"
                } else {
                    "暂无刷新任务"
                }));
            }
            let mut cancel = None;
            egui::ScrollArea::vertical()
                .max_height(280.)
                .show(ui, |ui| {
                    for task in &self.page.items {
                        ui.separator();
                        ui.strong(&task.provider_name);
                        ui.label(
                            task.roots
                                .iter()
                                .map(|r| r.name.as_str())
                                .collect::<Vec<_>>()
                                .join(" / "),
                        );
                        ui.horizontal(|ui| {
                            ui.weak(tr(match task.status.as_str() {
                                "queued" => "等待刷新",
                                "running" => "正在刷新",
                                "succeeded" => "刷新完成",
                                "failed" => "刷新未完成",
                                "cancelled" => "已取消",
                                "cancelling" => "正在中断…",
                                other => other,
                            }));
                            if self.all {
                                ui.small(&task.owner_name);
                            }
                            if task.cancellable
                                && ui
                                    .add_enabled(
                                        self.cancel.is_none(),
                                        egui::Button::new(tr("中断")),
                                    )
                                    .clicked()
                            {
                                cancel = Some(task.id.clone());
                            }
                        });
                    }
                });
            if let Some(id) = cancel {
                let server = svc.server.clone();
                let scope = if self.all { "all" } else { "mine" };
                self.cancel = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                    let id =
                        url::form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>();
                    let path = format!("/tasks/{id}/cancel?scope={scope}");
                    server
                        .settings_json("POST", &path, Some(&serde_json::json!({})), &[])
                        .await
                        .map(|_| ())
                }));
            }
            if self.page.next_cursor.is_some()
                && ui
                    .add_enabled(self.pending.is_none(), egui::Button::new(tr("加载更多")))
                    .clicked()
            {
                self.load(ui, svc, true);
            }
        });
    }
}
