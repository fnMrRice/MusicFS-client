use super::super::{
    actions::Actions,
    data::{play_row_version, Services},
    matching::Matching,
    style,
};
use super::providers::Providers;
use crate::{i18n::tr, icons, task::Task};
use eframe::egui;
use musicfs_server_adapter::{Playlist, Track};
use std::collections::BTreeMap;
use uuid::Uuid;

enum ResultKind {
    Revision(u64),
    Default,
    Favorite(String),
}
enum Choice {
    Play,
    Favorite(bool),
    Default,
    Add,
}

#[derive(Default)]
pub struct Versions {
    expanded: BTreeMap<usize, (String, usize)>,
    pending: Option<Task<ResultKind>>,
    revision: Option<u64>,
    error: Option<String>,
    retry: Option<(String, Option<String>, u64, String)>,
}
impl Versions {
    pub fn expanded(&self, rows: &[Track]) -> Vec<(usize, usize)> {
        self.expanded
            .iter()
            .filter_map(|(&i, (id, n))| rows.get(i).filter(|t| &t.id == id).map(|_| (i, *n)))
            .collect()
    }
    pub fn is_open(&self, i: usize) -> bool {
        self.expanded.contains_key(&i)
    }
    pub fn toggle(&mut self, ui: &egui::Ui, svc: Services<'_>, i: usize, track: &Track) {
        if self.expanded.remove(&i).is_some() {
            return;
        }
        let Some(group) = &track.sources else { return };
        self.expanded
            .insert(i, (track.id.clone(), group.choices().count()));
        if group.recording_id.is_some() && self.revision.is_none() && self.pending.is_none() {
            self.load_revision(ui, svc);
        }
    }
    fn load_revision(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        self.error = None;
        let server = svc.server.clone();
        self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            server.matching_revision().await.map(ResultKind::Revision)
        }));
    }
    pub fn poll(&mut self, ui: &mut egui::Ui, svc: Services<'_>, actions: &mut Actions) -> bool {
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(ResultKind::Revision(r)) => self.revision = Some(r),
                Ok(ResultKind::Favorite(id)) => {
                    actions.favorites.record(id, true);
                    return true;
                }
                Ok(ResultKind::Default) => return true,
                Err(e) => {
                    if e.contains("HTTP 409") || e.contains("HTTP 412") {
                        self.revision = None;
                        self.retry = None;
                    }
                    self.error = Some(e);
                }
            }
        }
        if let Some(error) = &self.error {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, error);
            });
            if self.revision.is_none() && self.pending.is_none() && ui.button(tr("刷新")).clicked()
            {
                self.load_revision(ui, svc);
            }
        }
        false
    }
    pub fn source(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        rows: &[Track],
        i: usize,
        index: usize,
        providers: &Providers,
        mut matching: Option<&mut Matching>,
        actions: &mut Actions,
        lists: &[Playlist],
    ) {
        let t = &rows[i];
        let Some(group) = &t.sources else { return };
        let Some(src) = group.choices().nth(index) else {
            return;
        };
        let selected = t.forced_source_track_id.as_ref() == Some(&src.source_track_id);
        let default = group.default.source_track_id == src.source_track_id;
        let manual = group.manual_source_track_id.as_ref() == Some(&src.source_track_id);
        let available = src.availability == "available";
        let busy = self.pending.is_some();
        let mut meta = vec![providers.label(&src.provider_instance_id).to_owned()];
        if let Some(codec) = src.container.as_ref().or(src.codec.as_ref()) {
            meta.push(codec.to_uppercase());
        }
        if let Some(hz) = src.sample_rate_hz {
            meta.push(format!("{} kHz", hz as f32 / 1000.));
        }
        if let Some(bps) = src.bitrate_bps {
            meta.push(format!("{} kbps", bps / 1000));
        }
        if default {
            meta.push(
                tr(if manual {
                    "手动默认"
                } else {
                    "自动默认"
                })
                .into(),
            );
        }
        if selected {
            meta.push(tr("已固定").into());
        }
        if !available {
            meta.push(tr("不可用").into());
        }
        let mut choice = None;
        ui.horizontal_centered(|ui| {
            ui.add_space(25.);
            let name_w = (ui.available_width() - 96.).max(40.);
            ui.allocate_ui_with_layout(
                egui::vec2(name_w, 48.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(name_w);
                    ui.add(
                        egui::Label::new(egui::RichText::new(&src.display_name).size(14.))
                            .truncate(),
                    )
                    .on_hover_text(&src.display_name);
                    let label = meta.join("  ·  ");
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&label)
                                .size(12.)
                                .color(style::muted(ui.visuals().dark_mode)),
                        )
                        .truncate(),
                    )
                    .on_hover_text(label);
                },
            );
            if ui
                .add_enabled_ui(available, |ui| {
                    icons::button(ui, "play", tr("播放此版本"), 30., 16.)
                })
                .inner
                .clicked()
            {
                choice = Some(Choice::Play);
            }
            let more = icons::button(ui, "more", tr("更多操作"), 30., 18.);
            let popup = more.id.with("source-menu");
            if more.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            egui::popup_below_widget(
                ui,
                popup,
                &more,
                egui::PopupCloseBehavior::CloseOnClick,
                |ui| {
                    ui.set_min_width(146.);
                    if ui
                        .add_enabled(available, egui::Button::new(tr("播放此版本")))
                        .clicked()
                    {
                        choice = Some(Choice::Play);
                    }
                    if ui.button(tr("加入歌单")).clicked() {
                        choice = Some(Choice::Add);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(tr("收藏此版本")))
                        .clicked()
                    {
                        choice = Some(Choice::Favorite(true));
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(tr("收藏并跟随默认来源")))
                        .clicked()
                    {
                        choice = Some(Choice::Favorite(false));
                    }
                    if group.recording_id.is_some()
                        && ui
                            .add_enabled(
                                !busy && self.revision.is_some(),
                                egui::Button::new(tr(if manual {
                                    "取消默认"
                                } else {
                                    "设为默认"
                                })),
                            )
                            .clicked()
                    {
                        choice = Some(Choice::Default);
                    }
                    if group.choices().count() > 1 {
                        if let Some(manager) = matching.as_deref_mut() {
                            manager.unlink_button(ui, src, t);
                        }
                    }
                },
            );
        });
        match choice {
            Some(Choice::Play) => {
                if let Err(e) =
                    play_row_version(svc.player, rows, i, Some(src.source_track_id.clone()))
                {
                    actions.notice = Some((e, true));
                }
            }
            Some(Choice::Add) => {
                let mut pinned = t.clone();
                pinned.forced_source_track_id = Some(src.source_track_id.clone());
                actions.open(&pinned, lists);
            }
            Some(Choice::Favorite(pinned)) => {
                let server = svc.server.clone();
                let pin = pinned.then(|| src.source_track_id.clone());
                let id = t.id.clone();
                self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                    server.favorite_version(&id, pin.as_deref()).await?;
                    Ok(ResultKind::Favorite(id))
                }));
            }
            Some(Choice::Default) => {
                let (Some(revision), Some(recording)) = (self.revision, group.recording_id.clone())
                else {
                    return;
                };
                let pin = (!manual).then(|| src.source_track_id.clone());
                let key = self
                    .retry
                    .as_ref()
                    .filter(|(id, p, r, _)| id == &recording && p == &pin && *r == revision)
                    .map(|(_, _, _, k)| k.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                self.retry = Some((recording.clone(), pin.clone(), revision, key.clone()));
                let server = svc.server.clone();
                self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                    server
                        .choose_default_source(&recording, pin.as_deref(), revision, &key)
                        .await?;
                    Ok(ResultKind::Default)
                }));
            }
            None => {}
        }
    }
}
