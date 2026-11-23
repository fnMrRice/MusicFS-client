use super::{request, segment, uncertain, Services};
use crate::{i18n::tr, task::Task};
use eframe::egui;
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Operation {
    operation_id: String,
    status: String,
    error_code: Option<String>,
}

#[derive(Clone)]
struct Change {
    path: String,
    enabled: bool,
    revision: String,
    key: String,
}

#[derive(Default)]
pub(super) struct Switches {
    change: Option<Change>,
    operation: Option<Operation>,
    pending: Option<Task<Operation>>,
    error: Option<String>,
    notice: Option<&'static str>,
    next_poll: Option<Instant>,
}

impl Switches {
    pub fn busy(&self) -> bool {
        self.change.is_some() || self.operation.is_some() || self.pending.is_some()
    }

    pub fn start(
        &mut self,
        ctx: &egui::Context,
        svc: Services<'_>,
        plugin: &str,
        instance: Option<&str>,
        enabled: bool,
        revision: &str,
    ) {
        if self.busy() {
            return;
        }
        let (group, id) = instance.map_or(("plugins", plugin), |id| ("plugin-instances", id));
        let id = match segment(id) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        self.notice = None;
        self.change = Some(Change {
            path: format!("/admin/{group}/{id}/state"),
            enabled,
            revision: revision.to_owned(),
            key: Uuid::new_v4().to_string(),
        });
        self.submit(ctx, svc);
    }

    fn submit(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let server = svc.server.clone();
        self.error = None;
        if let Some(op) = &self.operation {
            let id = op.operation_id.clone();
            self.pending = Some(Task::spawn(svc.runtime, ctx, async move {
                request(
                    &server,
                    "GET",
                    &format!("/admin/plugin-operations/{}", segment(&id)?),
                    None,
                    &[],
                )
                .await
            }));
        } else if let Some(change) = self.change.clone() {
            self.pending = Some(Task::spawn(svc.runtime, ctx, async move {
                let headers = [
                    ("If-Match", change.revision.as_str()),
                    ("Idempotency-Key", change.key.as_str()),
                ];
                request(
                    &server,
                    "PUT",
                    &change.path,
                    Some(&json!({"enabled":change.enabled})),
                    &headers,
                )
                .await
            }));
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> bool {
        let mut refresh = false;
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(op) => {
                    self.change = None;
                    self.operation = None;
                    self.next_poll = None;
                    match op.status.as_str() {
                        "succeeded" => {
                            self.notice = Some("设置已保存。");
                            refresh = true;
                        }
                        "applying" | "draining" => {
                            self.next_poll = Some(Instant::now() + Duration::from_secs(1));
                            self.operation = Some(op);
                        }
                        "failed" | "superseded" => {
                            let key = if op.status == "failed" {
                                "操作失败。"
                            } else {
                                "该操作已被替代。"
                            };
                            self.error = Some(format!(
                                "{} {}",
                                tr(key),
                                op.error_code.as_deref().unwrap_or_default()
                            ));
                            refresh = true;
                        }
                        _ => {
                            self.error = Some("Invalid MusicFS plugin operation status.".into());
                            refresh = true;
                        }
                    }
                }
                Err(e) => {
                    if !uncertain(&e) {
                        self.change = None;
                        self.operation = None;
                        refresh = true;
                    }
                    self.error = Some(e);
                }
            }
        }
        if let Some(msg) = self.notice {
            ui.label(tr(msg));
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if self.busy() {
                ui.horizontal(|ui| {
                    if ui.button(tr("重试上次操作")).clicked() {
                        self.submit(ui.ctx(), svc);
                    }
                    if ui.button(tr("刷新插件列表")).clicked() {
                        *self = Self::default();
                        refresh = true;
                    }
                });
            }
        } else if self.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(tr("正在应用…"));
            });
            if self.pending.is_none() {
                if let Some(time) = self.next_poll {
                    let delay = time.saturating_duration_since(Instant::now());
                    if delay.is_zero() {
                        self.submit(ui.ctx(), svc);
                    } else {
                        ui.ctx().request_repaint_after(delay);
                    }
                }
            }
        }
        refresh
    }
}
