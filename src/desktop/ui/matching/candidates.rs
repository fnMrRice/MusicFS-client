use super::{evidence::Evidence, Services};
use crate::{i18n::tr, task::Task};
use eframe::egui;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ref {
    source_track_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reason {
    kind: String,
    duration_difference_ms: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    candidate_id: String,
    left: Ref,
    right: Ref,
    left_track: Evidence,
    right_track: Evidence,
    score: u8,
    reasons: Vec<Reason>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    items: Vec<Candidate>,
    revision: u64,
    next_after: Option<String>,
}

pub(super) enum Choice {
    Merge(Vec<Evidence>, u64),
    Exclude(Vec<Evidence>, u64),
    Settings,
}

#[derive(Default)]
pub(super) struct Candidates {
    rows: Vec<Candidate>,
    seen: HashSet<String>,
    revision: Option<u64>,
    after: Option<String>,
    cursors: HashSet<String>,
    task: Option<Task<Page>>,
    error: Option<String>,
    stale: bool,
}
impl Candidates {
    fn load(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        let server = svc.server.clone();
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair("limit", "20");
        if let Some(after) = &self.after {
            query.append_pair("after", after);
        }
        let path = format!("/matching/candidates?{}", query.finish());
        self.error = None;
        self.task = Some(Task::spawn(svc.runtime, ctx, async move {
            let value = server.settings_json("GET", &path, None, &[]).await?;
            let page: Page =
                serde_json::from_value(value).map_err(|_| "Invalid matching candidates.")?;
            if page.items.iter().any(|item| {
                item.left.source_track_id != item.left_track.source_track_id
                    || item.right.source_track_id != item.right_track.source_track_id
                    || item.score > 100
            }) {
                return Err("Invalid matching evidence.".into());
            }
            Ok(page)
        }));
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        locked: bool,
        preview_error: &mut Option<String>,
        lookup: &dyn Fn(&str) -> String,
    ) -> Option<Choice> {
        let mut choice = None;
        if self.task.is_none() && self.error.is_none() && self.revision.is_none() {
            self.load(ui.ctx(), svc);
        }
        if let Some(result) = self.task.as_mut().and_then(Task::poll) {
            self.task = None;
            match result {
                Ok(page)
                    if self.revision.is_some_and(|rev| rev != page.revision)
                        || page
                            .next_after
                            .as_ref()
                            .is_some_and(|c| c.is_empty() || !self.cursors.insert(c.clone())) =>
                {
                    self.stale = true;
                    self.error = Some(tr("歌曲关联已发生变化，请刷新列表后重新选择。").into());
                }
                Ok(page) => {
                    self.revision = Some(page.revision);
                    self.after = page.next_after;
                    self.rows.extend(
                        page.items
                            .into_iter()
                            .filter(|c| self.seen.insert(c.candidate_id.clone())),
                    );
                }
                Err(e) => self.error = Some(e),
            }
        }
        ui.label(tr("插件自动找出疑似重复歌曲。确认属于同一录音后再合并；现场版、翻唱和不同录音可选择排除。"));
        ui.weak(tr("评分表示优先核对顺序，不是相同概率。"));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.task.is_none() && !locked,
                    egui::Button::new(tr("刷新候选")),
                )
                .clicked()
            {
                *self = Self::default();
                self.load(ui.ctx(), svc);
            }
            if svc.server.info.role == "admin" && ui.button(tr("打开插件设置")).clicked() {
                choice = Some(Choice::Settings);
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if !self.stale && ui.button(tr("重试")).clicked() {
                self.load(ui.ctx(), svc);
            }
        }
        for c in &self.rows {
            ui.push_id(&c.candidate_id, |ui| {
                if ui.available_width() > 390.0 {
                    ui.columns(2, |columns| {
                        c.left_track
                            .show(&mut columns[0], svc, preview_error, lookup);
                        c.right_track
                            .show(&mut columns[1], svc, preview_error, lookup);
                    });
                } else {
                    c.left_track.show(ui, svc, preview_error, lookup);
                    c.right_track.show(ui, svc, preview_error, lookup);
                }
                ui.horizontal_wrapped(|ui| {
                    ui.strong(tr("核对评分：{0}").replace("{0}", &c.score.to_string()));
                    for reason in &c.reasons {
                        ui.weak(tr(match reason.kind.as_str() {
                            "verifiedFileSHA512Equal" => "文件内容相同",
                            "fingerprintSimilar" => "音频指纹相近",
                            "titleEqual" => "曲名相近",
                            "artistOverlap" => "歌手相同",
                            "metadataOnly" => "曲目信息相近",
                            "durationClose" => "时长相近",
                            _ => "匹配证据",
                        }));
                        if let Some(ms) = reason.duration_difference_ms {
                            ui.weak(
                                tr("时长相差 {0} 秒")
                                    .replace("{0}", &format!("{:.1}", ms as f64 / 1000.)),
                            );
                        }
                    }
                });
                if let Some(rev) = self.revision {
                    ui.add_enabled_ui(!locked && !self.stale && self.task.is_none(), |ui| {
                        ui.horizontal(|ui| {
                            if ui.button(tr("合并歌曲")).clicked() {
                                choice = Some(Choice::Merge(
                                    vec![c.left_track.clone(), c.right_track.clone()],
                                    rev,
                                ));
                            }
                            if ui.button(tr("不是同一录音")).clicked() {
                                choice = Some(Choice::Exclude(
                                    vec![c.left_track.clone(), c.right_track.clone()],
                                    rev,
                                ));
                            }
                        })
                    });
                }
                ui.separator();
            });
        }
        if self.task.is_some() {
            ui.spinner();
        } else if self.after.is_some() && !self.stale && ui.button(tr("加载更多")).clicked() {
            self.load(ui.ctx(), svc);
        } else if self.rows.is_empty() && self.error.is_none() && self.after.is_none() {
            ui.weak(tr(if svc.server.info.role == "admin" {
                "暂无待确认的重复歌曲。可在插件设置中开启自动识别或立即识别。"
            } else {
                "暂无待确认的重复歌曲。"
            }));
        }
        choice
    }
}
