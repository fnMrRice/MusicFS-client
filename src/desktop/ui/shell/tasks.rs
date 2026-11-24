use super::{popup, Services};
use crate::{i18n::tr, icons, task::Task};
use eframe::egui::{self, Ui};
use musicfs_server_adapter::{
    tasks::{RefreshTask, TaskPage, TaskSnapshot, TaskUpdates},
    ServerSession,
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct Queue {
    page: TaskPage,
    all: bool,
    updates: Option<TaskUpdates>,
    waiting: bool,
    history: Vec<RefreshTask>,
    history_pages: usize,
    history_cursor: Option<String>,
    pending: Option<Task<TaskPage>>,
    changed_during_read: HashMap<String, RefreshTask>,
    cancel: Option<Task<()>>,
    error: Option<String>,
    subscription_error: Option<String>,
}

impl Queue {
    fn subscribe(&mut self, svc: Services<'_>) {
        self.updates = Some(svc.activity.subscribe_tasks(self.all));
        self.waiting = true;
        self.error = None;
        self.subscription_error = None;
    }

    fn apply(&mut self, snapshot: TaskSnapshot) -> bool {
        let previous = std::mem::replace(&mut self.page, snapshot.page);
        self.waiting = false;
        self.subscription_error = None;
        if self.page.next_cursor.is_none() {
            self.history_pages = 0;
        }
        let first: HashSet<_> = self.page.items.iter().map(|row| row.id.as_str()).collect();
        let overlap = previous
            .items
            .iter()
            .any(|row| first.contains(row.id.as_str()));
        if self.history_pages == 0 || snapshot.reset || !overlap {
            self.history.clear();
            self.history_cursor = None;
            self.pending = None;
            self.changed_during_read.clear();
            return self.history_pages > 0;
        }
        let changed: HashMap<_, _> = snapshot
            .changed
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect();
        if self.pending.is_some() {
            self.changed_during_read.extend(changed.clone());
        }
        // 新任务可能把首屏末尾的旧任务挤到历史区，保留已展开的连续范围。
        self.history = previous
            .items
            .into_iter()
            .chain(self.history.drain(..))
            .filter(|row| !first.contains(row.id.as_str()))
            .map(|row| changed.get(&row.id).cloned().unwrap_or(row))
            .collect();
        false
    }

    fn load_history(&mut self, ui: &Ui, svc: Services<'_>, cursor: String, pages: usize) {
        let server = svc.server.clone();
        let all = self.all;
        self.error = None;
        self.changed_during_read.clear();
        self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            read_history(&server, all, cursor, pages).await
        }));
    }

    fn append(&mut self, page: TaskPage) {
        let mut ids: HashSet<_> = self
            .page
            .items
            .iter()
            .chain(&self.history)
            .map(|row| row.id.clone())
            .collect();
        self.history.extend(
            page.items
                .into_iter()
                .filter(|row| ids.insert(row.id.clone()))
                .map(|row| self.changed_during_read.remove(&row.id).unwrap_or(row)),
        );
        self.changed_during_read.clear();
        self.history_cursor = page.next_cursor;
    }

    pub(super) fn button(&mut self, ui: &mut Ui, svc: Services<'_>) {
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(page) => self.append(page),
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(result) = self.cancel.as_mut().and_then(Task::poll) {
            self.cancel = None;
            if let Err(e) = result {
                self.error = Some(e);
            }
        }
        if self.updates.is_none() {
            self.subscribe(svc);
        }
        if let Some(update) = self.updates.as_mut().and_then(TaskUpdates::take) {
            match update {
                Ok(snapshot) => {
                    if self.apply(snapshot) {
                        if let Some(cursor) = self.page.next_cursor.clone() {
                            self.load_history(ui, svc, cursor, self.history_pages);
                        }
                    }
                }
                Err(error) => {
                    self.waiting = false;
                    self.subscription_error = Some(error);
                }
            }
        }
        let r = icons::button(ui, "queue", tr("任务队列"), 36., 22.);
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
                    *self = Self {
                        all: self.all,
                        ..Default::default()
                    };
                    self.subscribe(svc);
                }
            }
            if let Some(e) = self.subscription_error.as_ref().or(self.error.as_ref()) {
                ui.colored_label(ui.visuals().error_fg_color, e);
                if ui.button(tr("重试")).clicked() {
                    self.subscribe(svc);
                }
            } else if self.page.items.is_empty() {
                ui.weak(tr(if self.waiting {
                    "正在读取任务…"
                } else {
                    "暂无刷新任务"
                }));
            }
            let mut cancel = None;
            egui::ScrollArea::vertical()
                .max_height(280.)
                .show(ui, |ui| {
                    for task in self.page.items.iter().chain(&self.history) {
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
            let cursor = if self.history_pages == 0 {
                &self.page.next_cursor
            } else {
                &self.history_cursor
            };
            if let Some(cursor) = cursor.clone() {
                if ui
                    .add_enabled(self.pending.is_none(), egui::Button::new(tr("加载更多")))
                    .clicked()
                {
                    self.history_pages += 1;
                    self.load_history(ui, svc, cursor, 1);
                }
            }
        });
    }
}

async fn read_history(
    server: &ServerSession,
    all: bool,
    mut cursor: String,
    pages: usize,
) -> Result<TaskPage, String> {
    let scope = if all { "all" } else { "mine" };
    let mut result = TaskPage::default();
    let mut cursors = HashSet::new();
    for _ in 0..pages {
        if !cursors.insert(cursor.clone()) {
            return Err("Repeated MusicFS task cursor.".into());
        }
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("scope", scope)
            .append_pair("limit", "50")
            .append_pair("cursor", &cursor)
            .finish();
        let value = server
            .settings_json("GET", &format!("/tasks?{query}"), None, &[])
            .await?;
        let page: TaskPage =
            serde_json::from_value(value).map_err(|_| "Invalid MusicFS task page.")?;
        result.items.extend(page.items);
        result.next_cursor = page.next_cursor;
        let Some(next) = &result.next_cursor else {
            break;
        };
        if next.is_empty() || cursors.contains(next) {
            return Err("Invalid MusicFS task cursor.".into());
        }
        cursor = next.clone();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: &str) -> RefreshTask {
        RefreshTask {
            id: id.into(),
            status: status.into(),
            provider_name: String::new(),
            owner_name: String::new(),
            roots: vec![],
            cancellable: false,
        }
    }

    fn page(ids: &[&str]) -> TaskPage {
        TaskPage {
            items: ids.iter().map(|id| task(id, "queued")).collect(),
            next_cursor: Some("cursor".into()),
            active_count: 1,
        }
    }

    #[test]
    fn pushed_head_preserves_open_history_and_updates_old_rows() {
        let mut queue = Queue {
            page: page(&["b", "a"]),
            history: vec![task("old", "running")],
            history_pages: 1,
            history_cursor: Some("tail".into()),
            ..Default::default()
        };
        assert!(!queue.apply(TaskSnapshot {
            page: page(&["c", "b"]),
            changed: vec![task("old", "succeeded")],
            reset: false
        }));
        assert_eq!(
            queue
                .history
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "old"]
        );
        assert_eq!(queue.history[1].status, "succeeded");
        assert_eq!(queue.history_cursor.as_deref(), Some("tail"));
        assert!(queue.apply(TaskSnapshot {
            page: page(&["c", "b"]),
            changed: vec![],
            reset: true
        }));
        assert!(queue.history.is_empty());
        assert_eq!(queue.history_pages, 1);
    }
}
