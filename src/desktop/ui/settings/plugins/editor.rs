use super::{
    catalog::localized,
    request,
    schema::{Draft, Job, Settings},
    segment, uncertain, Services,
};
use crate::{i18n::tr, task::Task};
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
        let mut close = false;
        let mut reload = false;
        let mut retry = false;
        let mut submission = None;
        let busy = self.posting.is_some();
        let blocked = busy || self.loading.is_some() || self.conflict || self.pending.is_some();
        let modal = egui::Modal::new(egui::Id::new("plugin-settings-dialog")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 64.0).clamp(180.0, 480.0));
            ui.heading(tr("配置 {0}").replace("{0}", &self.name));
            ui.separator();
            egui::ScrollArea::vertical().max_height((ctx.screen_rect().height() - 145.0).max(120.0)).show(ui, |ui| {
                if self.loading.is_some() { ui.weak(tr("正在读取设置…")); }
                if let Some(e) = &self.error { ui.colored_label(ui.visuals().error_fg_color, e); }
                if self.loading.is_none() && (self.data.is_none() || self.conflict || self.pending.is_some()) {
                    ui.horizontal_wrapped(|ui| {
                        if self.pending.is_some() && !busy && ui.button(tr("重试上次操作")).clicked() { retry = true; }
                        reload = ui.add_enabled(!busy, egui::Button::new(tr("重新加载设置"))).clicked();
                    });
                }
                if let Some(data) = &self.data {
                    let schema = &data.schema;
                    ui.strong(localized(&schema.title, &schema.localized_titles));
                    let description = localized(&schema.description, &schema.localized_descriptions);
                    if !description.is_empty() { ui.weak(description); }
                    ui.add_enabled_ui(!blocked, |ui| {
                        for field in &schema.fields {
                            ui.push_id(&field.key, |ui| field.show(ui, &mut self.draft));
                        }
                    });
                    let dirty = self.draft != self.original;
                    let values = schema.values(&self.draft);
                    if !schema.fields.is_empty() && ui.add_enabled(!blocked && dirty && values.is_some(),
                        egui::Button::new(tr(if busy { "正在保存…" } else { "保存设置" }))).clicked() {
                        submission = Some(Submission::Save(json!({
                            "expectedRevision": data.revision, "schemaRevision": schema.revision, "values": values,
                        })));
                    }
                    if dirty && !schema.actions.is_empty() { ui.weak(tr("请先保存修改，再执行插件操作。")); }
                    let active = self.job.as_ref().is_some_and(Job::active);
                    for action in &schema.actions {
                        if ui.add_enabled(!blocked && !dirty && !active, egui::Button::new(action.text.label())).clicked() {
                            submission = Some(Submission::Action(action.id.clone()));
                        }
                        action.text.help(ui);
                    }
                }
                if let Some(message) = self.message { ui.label(tr(message)); }
                if let Some(job) = &self.job { job.show(ui); }
                if let Some(e) = &self.job_error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                    if ui.add_enabled(self.job_read.is_none(), egui::Button::new(tr("重试"))).clicked() {
                        self.job_revision = None; ctx.request_repaint();
                    }
                }
            });
            ui.separator();
            close = ui.add_enabled(!busy, egui::Button::new(tr("取消"))).clicked();
        });
        if let Some(value) = submission {
            self.pending = Some((value, Uuid::new_v4().to_string()));
            retry = true;
        }
        if retry {
            self.post(ctx, svc);
        } else if reload {
            self.reload(ctx, svc);
        }
        self.posting.is_none() && (close || modal.should_close())
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
