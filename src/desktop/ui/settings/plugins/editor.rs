mod dialog;

use super::{
    request,
    schema::{Draft, Job, Settings},
    segment, uncertain, Services,
};
use crate::{i18n::tr, task::Task};
use dialog::DialogAction;
use eframe::egui;
use musicfs_server_adapter::ServerSession;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Clone)]
enum Submission {
    Save(Value),
    Action(String),
}

enum Reply {
    Saved(Settings),
    Started(Job),
}

pub(super) struct Editor {
    plugin_id: String,
    name: String,
    data: Option<Settings>,
    draft: Draft,
    original: Draft,
    job: Option<Job>,
    loading: Option<Task<Settings>>,
    posting: Option<Task<Reply>>,
    job_read: Option<Task<Settings>>,
    job_revision: Option<u64>,
    pending: Option<(Submission, String)>,
    conflict: bool,
    error: Option<String>,
    job_error: Option<String>,
    message: Option<&'static str>,
}

impl Editor {
    pub fn new(plugin_id: &str, name: &str) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            name: name.into(),
            data: None,
            draft: Draft::new(),
            original: Draft::new(),
            job: None,
            loading: None,
            posting: None,
            job_read: None,
            job_revision: None,
            pending: None,
            conflict: false,
            error: None,
            job_error: None,
            message: None,
        }
    }

    fn read(&self, ctx: &egui::Context, svc: Services<'_>) -> Task<Settings> {
        let server = svc.server.clone();
        let plugin_id = self.plugin_id.clone();
        Task::spawn(svc.runtime, ctx, async move {
            let path = format!("/plugin-settings/{}", segment(&plugin_id)?);
            get_settings(&server, &path, &plugin_id, None).await
        })
    }

    fn reload(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        self.error = None;
        self.message = None;
        self.job_error = None;
        self.job_read = None;
        self.job_revision = Some(svc.activity.revisions().plugin_jobs);
        self.loading = Some(self.read(ctx, svc));
    }

    fn apply(&mut self, mut data: Settings) {
        self.job = data.job.take();
        self.original = data.draft();
        self.draft = self.original.clone();
        self.data = Some(data);
        self.conflict = false;
        self.pending = None;
        self.error = None;
    }

    fn post(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let Some((submission, key)) = self.pending.clone() else {
            return;
        };
        let server = svc.server.clone();
        let plugin_id = self.plugin_id.clone();
        self.job_read = None;
        self.job_revision = None;
        self.job_error = None;
        self.error = None;
        self.message = None;
        self.posting = Some(Task::spawn(svc.runtime, ctx, async move {
            let path = format!("/plugin-settings/{}", segment(&plugin_id)?);
            match submission {
                Submission::Save(body) => {
                    get_settings(&server, &path, &plugin_id, Some((&body, &key)))
                        .await
                        .map(Reply::Saved)
                }
                Submission::Action(action) => {
                    let path = format!("{path}/actions/{}", segment(&action)?);
                    let job = request(
                        &server,
                        "POST",
                        &path,
                        Some(&json!({})),
                        &[("Idempotency-Key", &key)],
                    )
                    .await?;
                    Ok(Reply::Started(job))
                }
            }
        }));
    }

    fn update(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let revision = svc.activity.revisions().plugin_jobs;
        if self.data.is_none() && self.loading.is_none() && self.job_revision != Some(revision) {
            self.reload(ctx, svc);
        }
        if let Some(result) = self.loading.as_mut().and_then(Task::poll) {
            self.loading = None;
            match result {
                Ok(data) => self.apply(data),
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(result) = self.posting.as_mut().and_then(Task::poll) {
            self.posting = None;
            match result {
                Ok(Reply::Saved(data)) => {
                    self.apply(data);
                    self.message = Some("设置已保存。");
                }
                Ok(Reply::Started(job)) => {
                    self.job = Some(job);
                    self.pending = None;
                    self.message = Some("任务已提交，可离开此页面，服务端会继续执行。");
                }
                Err(e) => {
                    self.conflict = e.contains("(HTTP 409)")
                        || e.contains("(HTTP 412)")
                        || e.contains("REVISION_CONFLICT");
                    if self.conflict || !uncertain(&e) {
                        self.pending = None;
                    }
                    self.error = Some(if self.conflict {
                        tr("插件设置已发生变化，请重新加载后再编辑。").to_owned()
                    } else if e.contains("INVALID_ARGUMENT") {
                        tr("配置内容无效，请检查填写格式后重试。").to_owned()
                    } else {
                        e
                    });
                }
            }
        }
        if let Some(result) = self.job_read.as_mut().and_then(Task::poll) {
            self.job_read = None;
            match result {
                Ok(data) => {
                    self.job = data.job;
                    self.job_error = None;
                }
                Err(e) => self.job_error = Some(e),
            }
        }
        if self.data.is_some()
            && self.loading.is_none()
            && self.posting.is_none()
            && self.job_read.is_none()
            && self.job_revision != Some(revision)
        {
            self.job_revision = Some(revision);
            self.job_read = Some(self.read(ctx, svc));
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, svc: Services<'_>) -> bool {
        self.update(ctx, svc);
        match self.dialog(ctx) {
            DialogAction::Close => return true,
            DialogAction::Reload => self.reload(ctx, svc),
            DialogAction::Retry => self.post(ctx, svc),
            DialogAction::RetryJob => {
                self.job_revision = None;
                ctx.request_repaint();
            }
            DialogAction::Submit(value) => {
                self.pending = Some((value, Uuid::new_v4().to_string()));
                self.post(ctx, svc);
            }
            DialogAction::None => {}
        }
        false
    }
}

async fn get_settings(
    server: &ServerSession,
    path: &str,
    plugin_id: &str,
    save: Option<(&Value, &str)>,
) -> Result<Settings, String> {
    let data: Settings = if let Some((body, key)) = save {
        request(server, "PUT", path, Some(body), &[("Idempotency-Key", key)]).await?
    } else {
        request(server, "GET", path, None, &[]).await?
    };
    if data.plugin_id != plugin_id {
        return Err("Invalid MusicFS plugin settings identity.".into());
    }
    Ok(data)
}

#[cfg(test)]
#[path = "../../../../../tests/native/plugin_dialog.rs"]
mod dialog_tests;
