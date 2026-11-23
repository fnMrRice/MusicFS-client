use super::super::{
    data::{play_row_version, Services},
    favorites::Favorites,
};
use crate::{i18n::tr, task::Task};
use eframe::egui;
use musicfs_server_adapter::Track;
use uuid::Uuid;

enum ResultKind {
    Revision(u64),
    Default,
    Favorite,
}
enum Choice {
    Play(String),
    Default(Option<String>),
    Favorite(Option<String>),
}

#[derive(Default)]
pub struct Versions {
    row: Option<(usize, String)>,
    revision: Option<u64>,
    pending: Option<Task<ResultKind>>,
    error: Option<String>,
    retry: Option<(Option<String>, u64, String)>,
}
impl Versions {
    pub fn open(&mut self, ui: &egui::Ui, svc: Services<'_>, i: usize, track: &Track) {
        if self.pending.is_some() {
            return;
        }
        *self = Self {
            row: Some((i, track.id.clone())),
            ..Default::default()
        };
        if track
            .sources
            .as_ref()
            .is_some_and(|g| g.recording_id.is_some())
        {
            self.load_revision(ui, svc);
        }
    }
    fn load_revision(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let server = svc.server.clone();
        self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            server.matching_revision().await.map(ResultKind::Revision)
        }));
    }
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        svc: Services<'_>,
        rows: &[Track],
        favs: &mut Favorites,
    ) -> bool {
        let Some((i, id)) = self.row.clone() else {
            return false;
        };
        let Some(t) = rows.get(i).filter(|t| t.id == id) else {
            return false;
        };
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(ResultKind::Revision(r)) => self.revision = Some(r),
                Ok(ResultKind::Default) => {
                    self.row = None;
                    return true;
                }
                Ok(ResultKind::Favorite) => {
                    favs.record(t.id.clone(), true);
                    self.row = None;
                    return true;
                }
                Err(e) => {
                    if e.contains("HTTP 409") || e.contains("HTTP 412") {
                        self.revision = None;
                        self.retry = None;
                    }
                    self.error = Some(e);
                }
            }
        }
        let mut open = true;
        let mut choice = None;
        egui::Window::new(tr("来源版本"))
            .id(egui::Id::new("source-versions"))
            .open(&mut open)
            .resizable(true)
            .default_width(460.)
            .show(ui.ctx(), |ui| {
                ui.heading(&t.title);
                let Some(group) = &t.sources else {
                    ui.weak(tr("暂无可用音源"));
                    return;
                };
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if self.revision.is_none()
                        && self.pending.is_none()
                        && group.recording_id.is_some()
                        && ui.button(tr("刷新")).clicked()
                    {
                        self.error = None;
                        self.load_revision(ui, svc);
                    }
                }
                if self.pending.is_some() {
                    ui.spinner();
                }
                egui::ScrollArea::vertical()
                    .max_height(370.)
                    .show(ui, |ui| {
                        for src in group.choices() {
                            ui.push_id(&src.source_track_id, |ui| {
                                ui.separator();
                                ui.horizontal_wrapped(|ui| {
                                    ui.strong(&src.display_name);
                                    if src.source_track_id == group.default.source_track_id {
                                        ui.weak(tr(
                                            if group.manual_source_track_id.as_ref()
                                                == Some(&src.source_track_id)
                                            {
                                                "手动默认"
                                            } else {
                                                "自动默认"
                                            },
                                        ));
                                    }
                                    if t.forced_source_track_id.as_ref()
                                        == Some(&src.source_track_id)
                                    {
                                        ui.weak(tr("已固定"));
                                    }
                                });
                                let mut meta = vec![src.provider_instance_id.clone()];
                                if let Some(codec) = src.container.as_ref().or(src.codec.as_ref()) {
                                    meta.push(codec.to_uppercase());
                                }
                                if let Some(rate) = src.sample_rate_hz {
                                    meta.push(format!("{} kHz", rate as f32 / 1000.));
                                }
                                if let Some(rate) = src.bitrate_bps {
                                    meta.push(format!("{} kbps", rate / 1000));
                                }
                                if src.availability != "available" {
                                    meta.push(tr("不可用").into());
                                }
                                ui.weak(meta.join("  ·  "));
                                ui.horizontal_wrapped(|ui| {
                                    if ui
                                        .add_enabled(
                                            src.availability == "available",
                                            egui::Button::new(tr("播放此版本")),
                                        )
                                        .clicked()
                                    {
                                        choice = Some(Choice::Play(src.source_track_id.clone()));
                                    }
                                    let available = self.pending.is_none();
                                    if ui
                                        .add_enabled(available, egui::Button::new(tr("收藏此版本")))
                                        .clicked()
                                    {
                                        choice = Some(Choice::Favorite(Some(
                                            src.source_track_id.clone(),
                                        )));
                                    }
                                    if group.recording_id.is_some() {
                                        let manual = group.manual_source_track_id.as_ref()
                                            == Some(&src.source_track_id);
                                        if ui
                                            .add_enabled(
                                                available && self.revision.is_some(),
                                                egui::Button::new(tr(if manual {
                                                    "取消默认"
                                                } else {
                                                    "设为默认"
                                                })),
                                            )
                                            .clicked()
                                        {
                                            choice = Some(Choice::Default(
                                                (!manual).then(|| src.source_track_id.clone()),
                                            ));
                                        }
                                    }
                                });
                            });
                        }
                    });
                ui.separator();
                if ui
                    .add_enabled(
                        self.pending.is_none(),
                        egui::Button::new(tr("收藏并跟随默认来源")),
                    )
                    .clicked()
                {
                    choice = Some(Choice::Favorite(None));
                }
            });
        match choice {
            Some(Choice::Play(source)) => {
                match play_row_version(svc.player, rows, i, Some(source)) {
                    Ok(()) => open = false,
                    Err(e) => self.error = Some(e),
                }
            }
            Some(Choice::Favorite(pin)) => {
                let server = svc.server.clone();
                self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                    server.favorite_version(&id, pin.as_deref()).await?;
                    Ok(ResultKind::Favorite)
                }));
            }
            Some(Choice::Default(pin)) => {
                if let (Some(revision), Some(recording)) = (
                    self.revision,
                    t.sources.as_ref().and_then(|s| s.recording_id.clone()),
                ) {
                    let key = self
                        .retry
                        .as_ref()
                        .filter(|(p, r, _)| p == &pin && *r == revision)
                        .map(|(_, _, k)| k.clone())
                        .unwrap_or_else(|| Uuid::new_v4().to_string());
                    self.retry = Some((pin.clone(), revision, key.clone()));
                    let server = svc.server.clone();
                    self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                        server
                            .choose_default_source(&recording, pin.as_deref(), revision, &key)
                            .await?;
                        Ok(ResultKind::Default)
                    }));
                }
            }
            None => {}
        }
        if !open && self.pending.is_none() {
            self.row = None;
        }
        false
    }
}
