use super::{
    api::{get, list, path_id, text, Load},
    Services,
};
use crate::i18n::tr;
use eframe::egui;
use musicfs_server_adapter::ServerSession;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use uuid::Uuid;

async fn submit(
    s: &ServerSession,
    instance: &str,
    root: Option<&str>,
    key: &str,
) -> Result<String, String> {
    let providers: Vec<Value> = list(s, "/provider-instances").await?;
    let p = providers
        .iter()
        .find(|p| text(p, "id") == instance)
        .ok_or("Music source is unavailable.")?;
    if !matches!(text(p, "availability"), "available" | "online")
        || !p["capabilities"]
            .as_array()
            .is_some_and(|a| a.iter().any(|c| c == "library.scan"))
    {
        return Err("Music source does not support scanning.".into());
    }
    let revision = p
        .pointer("/configuration/schemaRevision")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("Missing scan configuration.")?;
    let ids: Vec<_> = p["roots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["id"].as_str())
        .filter(|r| root.is_none_or(|id| id == *r))
        .collect();
    if ids.is_empty() {
        return Err("No matching music folders.".into());
    }
    let body = json!({"mode":"full","schemaRevision":revision,"rootIds":ids,"options":if root.is_some(){json!({"refreshMetadata":true})}else{json!({})}});
    let res = s
        .settings_json(
            "POST",
            &format!("/provider-instances/{}/scan-jobs", path_id(instance)),
            Some(&body),
            &[("Idempotency-Key", key)],
        )
        .await?;
    res["jobId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or("Invalid scan submission.".into())
}
pub(super) async fn auto_scan(
    s: &ServerSession,
    instance: &str,
    operation: &str,
) -> Result<String, String> {
    submit(s, instance, None, operation).await
}

#[derive(Default)]
pub(super) struct Scans {
    current: Load<Value>,
    discovered: bool,
    jobs: Vec<Job>,
}
struct Job {
    name: String,
    root: Option<String>,
    key: String,
    receipt: Load<String>,
    id: Option<String>,
    view: Load<(Value, Summary)>,
    next: Option<Instant>,
    done: bool,
    metadata: bool,
}
#[derive(Default)]
struct Summary {
    total: u64,
    completed: u64,
    applied: u64,
    skipped: u64,
    failed: u64,
}
impl Scans {
    pub fn refresh(
        &mut self,
        ui: &egui::Ui,
        svc: Services<'_>,
        instance: &str,
        root: &str,
        name: &str,
    ) {
        if self
            .jobs
            .iter()
            .any(|j| !j.done && j.root.as_deref() == Some(root))
        {
            return;
        }
        let mut job = Job::new(name.into(), Some(root.into()), true);
        job.submit(ui, svc, instance);
        self.jobs.push(job);
    }
    pub fn active(&self, root: &str) -> bool {
        self.jobs
            .iter()
            .any(|j| !j.done && j.root.as_deref() == Some(root))
    }
    pub fn monitor(&mut self, id: String) {
        if self.jobs.iter().any(|j| j.id.as_ref() == Some(&id)) {
            return;
        }
        let mut j = Job::new(String::new(), None, false);
        j.id = Some(id);
        self.jobs.push(j);
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        instance: &str,
        refresh: bool,
    ) -> bool {
        if self.current.poll() {
            self.discovered = true;
            if let Some(id) = self
                .current
                .value
                .as_ref()
                .and_then(|v| v.pointer("/job/jobId"))
                .and_then(Value::as_str)
            {
                self.monitor(id.to_owned());
            }
        }
        if !self.discovered && self.current.initial() || self.current.status(ui) || refresh {
            let (s, id) = (svc.server.clone(), path_id(instance));
            self.current.start(ui, svc, async move {
                get(&s, &format!("/provider-instances/{id}/scan-status")).await
            });
        }
        let mut finished = false;
        for job in &mut self.jobs {
            ui.push_id(egui::Id::new(&job.key), |ui| {
                finished |= job.show(ui, svc, instance);
            });
        }
        finished
    }
}
impl Job {
    fn new(name: String, root: Option<String>, metadata: bool) -> Self {
        Self {
            name,
            root,
            metadata,
            key: Uuid::new_v4().to_string(),
            receipt: Default::default(),
            id: None,
            view: Default::default(),
            next: None,
            done: false,
        }
    }
    fn submit(&mut self, ui: &egui::Ui, svc: Services<'_>, instance: &str) {
        let (s, id, root, key) = (
            svc.server.clone(),
            instance.to_owned(),
            self.root.clone(),
            self.key.clone(),
        );
        self.receipt.start(ui, svc, async move {
            submit(&s, &id, root.as_deref(), &key).await
        });
    }
    fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>, instance: &str) -> bool {
        if self.receipt.poll() {
            self.id = self.receipt.value.take();
        }
        if self.id.is_none() {
            ui.label(format!("{} · {}", self.name, tr("正在提交并确认扫描任务…")));
            if self.receipt.status(ui) {
                self.submit(ui, svc, instance);
            }
            return false;
        }
        let completed = self.view.poll();
        if completed {
            let (v, metadata) = self.view.value.as_ref().unwrap();
            self.done = !matches!(text(v, "status"), "queued" | "retry_wait" | "running")
                && metadata.completed == metadata.total;
            self.next = Some(Instant::now() + Duration::from_secs(2));
        }
        let retry = self.view.status(ui);
        if retry
            || !self.done
                && self.view.error.is_none()
                && !self.view.busy()
                && self.next.is_none_or(|n| Instant::now() >= n)
        {
            let (s, id, instance, metadata) = (
                svc.server.clone(),
                self.id.clone().unwrap(),
                instance.to_owned(),
                self.metadata,
            );
            self.view.start(ui, svc, async move {
                let v: Value = get(&s, &format!("/scan-jobs/{}", path_id(&id))).await?;
                if text(&v, "jobId") != id || text(&v, "providerInstanceId") != instance {
                    return Err("Unexpected scan job.".into());
                }
                let m = if metadata && text(&v, "status") == "succeeded" && v["complete"] == true {
                    summary(&s, &id).await?
                } else {
                    Summary::default()
                };
                Ok((v, m))
            });
        }
        if let Some((v, m)) = &self.view.value {
            let status = tr(match text(v, "status") {
                "queued" => "扫描已排队",
                "running" => "正在扫描音乐",
                "retry_wait" => "正在等待重试",
                "succeeded" => "扫描完成",
                "failed" => "扫描失败",
                "paused" => "扫描已暂停",
                "cancelled" => "扫描已取消",
                s => s,
            });
            let count = v
                .pointer("/progress/observed")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            ui.label(format!(
                "{}{}{} · {} {}",
                self.name,
                if self.name.is_empty() { "" } else { " · " },
                status,
                count,
                tr("首")
            ));
            if let Some(code) = v.pointer("/error/code").and_then(Value::as_str) {
                ui.colored_label(ui.visuals().error_fg_color, code);
            }
            if m.total > 0 {
                ui.weak(format!(
                    "{} {}/{} · {} {} · {} {} · {} {}",
                    tr("封面和歌词"),
                    m.completed,
                    m.total,
                    tr("已更新"),
                    m.applied,
                    tr("跳过"),
                    m.skipped,
                    tr("失败"),
                    m.failed
                ));
            }
        }
        completed && self.done
    }
}
async fn summary(s: &ServerSession, id: &str) -> Result<Summary, String> {
    let mut result = Summary::default();
    let mut cursor = String::new();
    let mut seen = HashSet::new();
    let mut triggers = HashSet::new();
    loop {
        let mut path = format!("/scan-jobs/{}/metadata-results?limit=200", path_id(id));
        if !cursor.is_empty() {
            path.push_str("&cursor=");
            path.push_str(&path_id(&cursor));
        }
        let page: Value = get(s, &path).await?;
        for row in page["items"]
            .as_array()
            .ok_or("Invalid metadata results.")?
        {
            if text(row, "scanJobId") != id || !triggers.insert(text(row, "triggerId").to_owned()) {
                return Err("Invalid metadata result.".into());
            }
            if text(row, "reason") != "refreshMetadata" {
                continue;
            }
            let actions = row["actions"]
                .as_array()
                .ok_or("Invalid metadata actions.")?;
            result.total += 1;
            if !matches!(text(row, "state"), "queued" | "preparing" | "running")
                && !(text(row, "state") == "completed"
                    && actions.iter().any(|a| a["status"] == "pending"))
            {
                result.completed += 1;
            }
            result.applied += actions.iter().filter(|a| a["status"] == "applied").count() as u64;
            result.skipped += actions.iter().filter(|a| a["status"] == "skipped").count() as u64;
            if row["state"] == "failed"
                || !row["failureCode"].is_null()
                || actions.iter().any(|a| a["status"] == "failed")
            {
                result.failed += 1;
            }
        }
        let Some(c) = page["nextCursor"].as_str() else {
            return Ok(result);
        };
        if c.is_empty() || !seen.insert(c.to_owned()) {
            return Err("Repeated metadata cursor.".into());
        }
        cursor = c.into();
    }
}
