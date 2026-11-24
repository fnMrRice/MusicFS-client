use super::Services;
use crate::{i18n::tr, task::Task};
use eframe::egui;
use musicfs_server_adapter::{ServerSession, SessionInfo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum Decision {
    Link {
        sources: Vec<String>,
        target: Option<String>,
    },
    Unlink(String),
    Exclude {
        left: String,
        right: String,
        excluded: bool,
    },
    Bulk {
        score: Option<u8>,
    },
    QueueBulk {
        score: Option<u8>,
    },
    QueueAlbums {
        provider: Option<String>,
        query: String,
    },
}

impl Decision {
    fn payload(&self, rev: u64) -> (&'static str, Value) {
        match self {
            Self::Link { sources, target } => (
                "links",
                json!({"expectedRevision":rev,"sourceTrackIds":sources,"targetRecordingId":target}),
            ),
            Self::Unlink(id) => (
                "unlinks",
                json!({"expectedRevision":rev,"sourceTrackId":id}),
            ),
            Self::Exclude {
                left,
                right,
                excluded,
            } => (
                "exclusions",
                json!({"expectedRevision":rev,"leftSourceTrackId":left,"rightSourceTrackId":right,"excluded":excluded}),
            ),
            Self::QueueAlbums { provider, query } => {
                let mut body = json!({});
                if let Some(provider) = provider {
                    body["providerInstanceId"] = json!(provider);
                }
                if !query.is_empty() {
                    body["q"] = json!(query);
                }
                ("album-scrape", body)
            }
            Self::Bulk { score: Some(score) } | Self::QueueBulk { score: Some(score) } => (
                "bulk-merge",
                json!({"expectedRevision":rev,"mode":"score","minimumScore":score}),
            ),
            Self::Bulk { score: None } | Self::QueueBulk { score: None } => (
                "bulk-merge",
                json!({"expectedRevision":rev,"mode":"title_artist"}),
            ),
        }
    }

    fn queued_kind(&self) -> Option<&'static str> {
        match self {
            Self::QueueBulk { .. } => Some("bulkMerge"),
            Self::QueueAlbums { .. } => Some("albumAutoScrape"),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Pending {
    endpoint: String,
    instance: String,
    user: String,
    revision: u64,
    key: UuidKey,
    request: Decision,
}

// 只允许以 UUID 作为日志文件名。
#[derive(Serialize, Deserialize, Clone)]
#[serde(try_from = "String", into = "String")]
struct UuidKey(Uuid);
impl TryFrom<String> for UuidKey {
    type Error = uuid::Error;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Uuid::parse_str(&s).map(Self)
    }
}
impl From<UuidKey> for String {
    fn from(k: UuidKey) -> Self {
        k.0.to_string()
    }
}

fn directory() -> Option<PathBuf> {
    std::env::var_os("MUSICFS_DESKTOP_CONFIG_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            directories::ProjectDirs::from("io.github", "MusicFS", "MusicFS Desktop")
                .map(|d| d.config_dir().to_owned())
        })
        .map(|p| p.join("matching"))
}

impl Pending {
    fn path(&self) -> Option<PathBuf> {
        Some(directory()?.join(format!("{}.json", self.key.0)))
    }

    fn persist(&self) -> std::io::Result<()> {
        let dir = directory().ok_or(std::io::ErrorKind::NotFound)?;
        fs::create_dir_all(&dir)?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(file.as_file_mut(), self)?;
        file.as_file_mut().sync_all()?;
        file.persist_noclobber(self.path().ok_or(std::io::ErrorKind::NotFound)?)?;
        Ok(())
    }

    fn restore(scope: &SessionInfo) -> Option<Self> {
        for file in fs::read_dir(directory()?).ok()?.flatten() {
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json")
                || !file.file_type().is_ok_and(|kind| kind.is_file())
                || !file.metadata().is_ok_and(|s| s.len() <= 64 * 1024)
            {
                continue;
            }
            let Ok(raw) = fs::read(&path) else {
                continue;
            };
            let Ok(p) = serde_json::from_slice::<Self>(&raw) else {
                continue;
            };
            if p.endpoint == scope.endpoint
                && p.instance == scope.instance_id
                && p.user == scope.user_name
                && p.path().as_ref() == Some(&path)
            {
                return Some(p);
            }
        }
        None
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Receipt {
    pub revision: u64,
    operation_id: String,
    pub merged_groups: Option<u64>,
    pub merged_sources: Option<u64>,
    pub skipped_groups: Option<u64>,
}

enum Submission {
    Queued,
    Applied(Receipt),
}

#[derive(Default)]
pub(super) struct Decisions {
    initialized: bool,
    pending: Option<Pending>,
    task: Option<Task<Submission>>,
    pub error: Option<String>,
    pub conflict: bool,
    receipt: Option<Submission>,
}

impl Decisions {
    pub fn locked(&self) -> bool {
        self.pending.is_some() || self.conflict
    }
    pub fn busy(&self) -> bool {
        self.task.is_some()
    }

    pub fn start(
        &mut self,
        ctx: &egui::Context,
        svc: Services<'_>,
        revision: u64,
        request: Decision,
    ) {
        if self.locked() {
            return;
        }
        let info = &svc.server.info;
        let pending = Pending {
            endpoint: info.endpoint.clone(),
            instance: info.instance_id.clone(),
            user: info.user_name.clone(),
            revision,
            key: UuidKey(Uuid::new_v4()),
            request,
        };
        self.receipt = None;
        // 写入失败时仍可提交并在本次会话内以原请求重试。
        let _ = pending.persist();
        self.pending = Some(pending);
        self.retry(ctx, svc);
    }

    fn retry(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let Some(p) = self.pending.clone() else {
            return;
        };
        if self.busy() {
            return;
        }
        self.error = None;
        let server = svc.server.clone();
        self.task = Some(Task::spawn(svc.runtime, ctx, async move {
            if let Some(expected) = p.request.queued_kind() {
                let (route, body) = p.request.payload(p.revision);
                let value = server
                    .settings_json(
                        "POST",
                        &format!("/tasks/{route}"),
                        Some(&body),
                        &[("Idempotency-Key", &p.key.0.to_string())],
                    )
                    .await?;
                if value["kind"] != expected || value["id"].as_str().is_none_or(str::is_empty) {
                    return Err("Invalid task submission result.".into());
                }
                Ok(Submission::Queued)
            } else {
                submit(&server, &p).await.map(Submission::Applied)
            }
        }));
    }

    fn clear(&mut self) {
        if let Some(path) = self.pending.take().and_then(|p| p.path()) {
            let _ = fs::remove_file(path);
        }
    }

    pub fn update(&mut self, svc: Services<'_>) -> bool {
        if !self.initialized {
            self.initialized = true;
            self.pending = Pending::restore(&svc.server.info);
        }
        let Some(result) = self.task.as_mut().and_then(Task::poll) else {
            return false;
        };
        self.task = None;
        match result {
            Ok(receipt) => {
                self.clear();
                self.receipt = Some(receipt);
                true
            }
            Err(e) => {
                self.conflict = [
                    "(HTTP 409)",
                    "(HTTP 410)",
                    "(HTTP 412)",
                    "REVISION_CONFLICT",
                    "IDEMPOTENCY_EXPIRED",
                ]
                .iter()
                .any(|s| e.contains(s));
                if self.conflict
                    || ["400", "401", "403", "404", "422"]
                        .iter()
                        .any(|code| e.contains(&format!("(HTTP {code})")))
                {
                    self.clear();
                }
                self.error = Some(if self.conflict {
                    tr(if e.contains("410") || e.contains("IDEMPOTENCY_EXPIRED") {
                        "上次操作的确认记录已过期，请刷新曲库检查结果后重新选择。"
                    } else {
                        "歌曲关联已发生变化，请刷新列表后重新选择。"
                    })
                    .to_owned()
                } else {
                    e
                });
                false
            }
        }
    }

    pub fn feedback(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> bool {
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if let Some(p) = &self.pending {
            if self.busy() {
                ui.weak(tr(if p.request.queued_kind().is_some() {
                    "正在加入任务队列…"
                } else if matches!(p.request, Decision::Bulk { .. }) {
                    "正在合并重复音乐，可离开此页面。"
                } else {
                    "正在保存歌曲关联，可离开此页面。"
                }));
            } else {
                let (hint, label) = if p.request.queued_kind().is_some() {
                    ("尚未确认入队结果，请重试原请求。", "继续确认入队结果")
                } else {
                    (
                        "上次歌曲关联操作的结果尚未确认，请继续确认。",
                        "继续确认关联结果",
                    )
                };
                ui.label(tr(hint));
                if ui.button(tr(label)).clicked() {
                    self.retry(ui.ctx(), svc);
                }
            }
        }
        if self.conflict && ui.button(tr("刷新列表并重新选择")).clicked() {
            self.conflict = false;
            self.error = None;
            return true;
        }
        if let Some(Submission::Queued) = &self.receipt {
            ui.label(tr("已加入任务队列"));
        }
        if let Some(Submission::Applied(r)) = &self.receipt {
            if let (Some(groups), Some(sources), Some(skipped)) =
                (r.merged_groups, r.merged_sources, r.skipped_groups)
            {
                if groups == 0 {
                    ui.label(tr("没有可合并的重复音乐。"));
                } else {
                    ui.label(
                        tr("已合并 {0} 组重复音乐，涉及 {1} 个来源版本。")
                            .replace("{0}", &groups.to_string())
                            .replace("{1}", &sources.to_string()),
                    );
                }
                if skipped > 0 {
                    ui.weak(
                        tr("已跳过 {0} 个存在排除、冲突或超限的分组。")
                            .replace("{0}", &skipped.to_string()),
                    );
                }
            }
        }
        false
    }
}

async fn submit(server: &ServerSession, p: &Pending) -> Result<Receipt, String> {
    let (kind, body) = p.request.payload(p.revision);
    let key = p.key.0.to_string();
    let value = server
        .settings_json(
            "POST",
            &format!("/matching/{kind}"),
            Some(&body),
            &[("Idempotency-Key", &key)],
        )
        .await?;
    let r: Receipt =
        serde_json::from_value(value).map_err(|_| "Invalid matching decision result.")?;
    if r.operation_id.is_empty()
        || r.revision < p.revision
        || (kind == "bulk-merge"
            && [r.merged_groups, r.merged_sources, r.skipped_groups].contains(&None))
    {
        return Err("Incomplete matching decision result.".into());
    }
    Ok(r)
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;
