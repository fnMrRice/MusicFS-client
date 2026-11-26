mod candidates;
mod decision;
mod evidence;
mod forms;

use super::data::Services;
use crate::{i18n::tr, task::Task};
use candidates::{Candidates, Choice};
use decision::{Decision, Decisions};
use eframe::egui;
use evidence::Evidence;
use forms::{exclusion, merge, Dialog};
use musicfs_server_adapter::{SourceVersion, Track};

#[derive(Default)]
pub(super) struct Matching {
    selected: Vec<Track>,
    revision: Option<u64>,
    request: Option<Task<u64>>,
    error: Option<String>,
    preview_error: Option<String>,
    decisions: Decisions,
    candidates: Candidates,
    dialog: Option<Dialog>,
    plugin_settings: bool,
}

fn row_key(t: &Track) -> (&str, Option<&str>) {
    (&t.id, t.forced_source_track_id.as_deref())
}

impl Matching {
    pub fn selected_tracks(&self) -> &[Track] {
        &self.selected
    }
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }
    pub fn take_plugin_settings(&mut self) -> bool {
        std::mem::take(&mut self.plugin_settings)
    }

    pub fn row(&mut self, ui: &mut egui::Ui, track: &Track) {
        let position = self
            .selected
            .iter()
            .position(|t| row_key(t) == row_key(track));
        let mut checked = position.is_some();
        let response = ui.add_enabled(
            !self.decisions.locked(),
            egui::Checkbox::without_text(&mut checked),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                ui.is_enabled(),
                checked,
                tr("选择 {0}").replace("{0}", &track.title),
            )
        });
        if response
            .on_hover_text(tr("选择 {0}").replace("{0}", &track.title))
            .changed()
        {
            if let Some(index) = position {
                self.selected.remove(index);
            } else {
                self.selected.push(track.clone());
            }
        }
    }

    pub fn unlink_button(&mut self, ui: &mut egui::Ui, src: &SourceVersion, track: &Track) -> bool {
        let clicked = ui
            .add_enabled(!self.decisions.locked(), egui::Button::new(tr("解除关联")))
            .clicked();
        if clicked {
            self.dialog = Some(Dialog::Review {
                action: Decision::Unlink(src.source_track_id.clone()),
                items: vec![Evidence::version(track, src)],
                revision: self.revision,
                from_candidates: false,
            });
        }
        clicked
    }

    fn revision(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        self.error = None;
        let server = svc.server.clone();
        self.request = Some(Task::spawn(svc.runtime, ctx, async move {
            server.matching_revision().await
        }));
    }

    pub fn update(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        lookup: &dyn Fn(&str) -> String,
    ) -> bool {
        let ctx = ui.ctx().clone();
        if self.request.is_none() && self.revision.is_none() && self.error.is_none() {
            self.revision(&ctx, svc);
        }
        if let Some(result) = self.request.as_mut().and_then(Task::poll) {
            self.request = None;
            match result {
                Ok(rev) => self.revision = Some(rev),
                Err(e) => self.error = Some(e),
            }
        }
        let mut refresh = self.decisions.update(svc);
        if self.dialog.is_none() {
            refresh |= self.decisions.feedback(ui, svc);
        }
        if refresh {
            self.changed(&ctx, svc);
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button(tr("重试")).clicked() {
                self.revision(&ctx, svc);
            }
        }
        if self.dialog.is_some() {
            refresh |= self.modal(&ctx, svc, lookup);
        }
        refresh
    }

    pub(super) fn queue_albums(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        provider: Option<&str>,
        query: &str,
    ) {
        let updated = self.decisions.update(svc);
        if self.decisions.feedback(ui, svc) || updated {
            self.changed(ui.ctx(), svc);
        }
        if ui
            .add_enabled(!self.locked(), egui::Button::new(tr("一键自动刮削")))
            .on_hover_text(tr("自动刮削当前来源和搜索条件下的所有未刮削专辑"))
            .clicked()
        {
            self.decisions.start(
                ui.ctx(),
                svc,
                0,
                Decision::QueueAlbums {
                    provider: provider.map(str::to_owned),
                    query: query.to_owned(),
                },
            );
        }
    }
    pub(super) fn buttons(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        if ui
            .add_enabled(!self.locked(), egui::Button::new(tr("一键合并重复音乐")))
            .clicked()
        {
            self.dialog = Some(Dialog::Bulk {
                by_score: true,
                minimum: "80".into(),
                preview: false,
            });
            if self.request.is_none() {
                self.revision(ui.ctx(), svc);
            }
        }
        if ui.button(tr("重复识别")).clicked() {
            self.dialog = Some(Dialog::Candidates);
        }
    }

    pub(super) fn selection_buttons(&mut self, ui: &mut egui::Ui, allow_manual: bool) {
        if !self.selected.is_empty() {
            let editable = self.selected.len() >= 2 && !self.decisions.locked();
            if allow_manual
                && ui
                    .add_enabled(editable, egui::Button::new(tr("合并歌曲")))
                    .clicked()
            {
                let items = evidence::selected(&self.selected);
                self.dialog = Some(merge(items, self.revision, false));
                ui.close_menu();
            }
            if self.selected.len() == 2 && !self.decisions.locked() {
                let left = self.selected[0].source();
                let right = self.selected[1].source();
                if let (Some(a), Some(b)) = (left, right) {
                    if a.source_track_id != b.source_track_id {
                        for (excluded, label) in [(true, "不是同一录音"), (false, "撤销排除")]
                        {
                            if ui.button(tr(label)).clicked() {
                                self.dialog = Some(exclusion(
                                    vec![
                                        Evidence::version(&self.selected[0], a),
                                        Evidence::version(&self.selected[1], b),
                                    ],
                                    self.revision,
                                    excluded,
                                    false,
                                ));
                                ui.close_menu();
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn locked(&self) -> bool {
        self.decisions.locked()
    }
    pub fn selected(&self, track: &Track) -> bool {
        self.selected.iter().any(|t| row_key(t) == row_key(track))
    }
    pub fn select_rows(&mut self, rows: &[Track], checked: bool) {
        if self.locked() {
            return;
        }
        let mut ids: std::collections::HashSet<_> = rows.iter().map(row_key).collect();
        if checked {
            for t in &self.selected {
                ids.remove(&row_key(t));
            }
            let added: Vec<_> = rows
                .iter()
                .filter(|t| ids.remove(&row_key(t)))
                .cloned()
                .collect();
            self.selected.extend(added);
        } else {
            self.selected.retain(|t| !ids.contains(&row_key(t)));
        }
    }

    fn changed(&mut self, ctx: &egui::Context, svc: Services<'_>) {
        self.clear_selection();
        self.candidates = Candidates::default();
        self.dialog = match self.dialog.take() {
            Some(
                Dialog::Candidates
                | Dialog::Review {
                    from_candidates: true,
                    ..
                },
            ) => Some(Dialog::Candidates),
            _ => None,
        };
        self.preview_error = None;
        self.revision(ctx, svc);
    }

    fn modal(
        &mut self,
        ctx: &egui::Context,
        svc: Services<'_>,
        lookup: &dyn Fn(&str) -> String,
    ) -> bool {
        let mut dialog = self.dialog.take().unwrap();
        let mut close = false;
        let mut reload = false;
        let mut choice = None;
        let mut submit = None;
        let mut review_again = false;
        let locked = self.decisions.locked();
        let revision = self.revision;
        let modal = egui::Modal::new(egui::Id::new("matching-dialog")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 70.0).clamp(180.0, 680.0));
            ui.heading(dialog.title());
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("matching-dialog-contents")
                .max_height((ctx.screen_rect().height() - 175.0).max(120.0))
                .show(ui, |ui| {
                    review_again = self.decisions.feedback(ui, svc);
                    if let Some(e) = &self.preview_error {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                    let ready = !locked && self.request.is_none() && self.error.is_none();
                    match &mut dialog {
                        Dialog::Candidates => {
                            choice = self.candidates.show(
                                ui,
                                svc,
                                locked,
                                &mut self.preview_error,
                                lookup,
                            )
                        }
                        Dialog::Review {
                            action,
                            items,
                            revision: captured,
                            ..
                        } => {
                            let rev = captured.or(revision);
                            if forms::review(
                                ui,
                                svc,
                                lookup,
                                action,
                                items,
                                ready && rev.is_some(),
                                &mut self.preview_error,
                            ) {
                                submit = rev.map(|r| (r, action.clone()));
                            }
                        }
                        Dialog::Bulk {
                            by_score,
                            minimum,
                            preview,
                        } => {
                            let enabled = ready && revision.is_some();
                            if let Some(action) =
                                forms::bulk(ui, by_score, minimum, preview, enabled)
                            {
                                submit = revision.map(|rev| (rev, action));
                            }
                        }
                    }
                    if let Some(e) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                        reload = ui.button(tr("重试")).clicked();
                    }
                });
            ui.separator();
            close = ui.button(tr("关闭")).clicked();
        });
        self.dialog = if close || modal.should_close() {
            if matches!(
                dialog,
                Dialog::Review {
                    from_candidates: true,
                    ..
                }
            ) {
                Some(Dialog::Candidates)
            } else {
                None
            }
        } else {
            Some(dialog)
        };
        if let Some((rev, request)) = submit {
            self.decisions.start(ctx, svc, rev, request);
        }
        if let Some(choice) = choice {
            self.dialog = match choice {
                Choice::Settings => {
                    self.plugin_settings = true;
                    None
                }
                Choice::Merge(items, rev) => Some(merge(items, Some(rev), true)),
                Choice::Exclude(items, rev) => Some(exclusion(items, Some(rev), true, true)),
            };
        }
        if review_again {
            self.changed(ctx, svc);
        } else if reload {
            self.revision(ctx, svc);
        }
        review_again
    }
}
