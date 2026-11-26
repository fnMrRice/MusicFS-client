use crate::{i18n::tr, task::Task, ui::data::Services};
use eframe::egui;
use musicfs_server_adapter::Track;
use serde_json::{json, Value};

const PAGE: usize = 8;

#[derive(Default)]
pub(super) struct Entries {
    tracks: Vec<Track>,
    pending: Option<Task<Vec<Track>>>,
    error: Option<String>,
}

impl Entries {
    fn load(&mut self, ui: &egui::Ui, svc: Services<'_>, entries: &[Value]) {
        let server = svc.server.clone();
        let rows = entries
            .iter()
            .skip(self.tracks.len())
            .take(PAGE)
            .cloned()
            .collect();
        self.error = None;
        self.pending = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            server.playlist_rule_entries(rows).await
        }));
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        entries: &mut Vec<Value>,
    ) -> bool {
        if entries.is_empty() {
            return false;
        }
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(tracks) => self.tracks.extend(tracks),
                Err(error) => self.error = Some(error),
            }
        }
        if self.tracks.is_empty() && self.pending.is_none() && self.error.is_none() {
            self.load(ui, svc, entries);
        }
        let end = if self.pending.is_some() || self.error.is_some() {
            (self.tracks.len() + PAGE).min(entries.len())
        } else {
            self.tracks.len()
        };
        let mut removed = None;
        let mut changed = false;
        for (index, entry) in entries.iter_mut().take(end).enumerate() {
            let track = self.tracks.get(index);
            let source = &mut entry["source"];
            let title = track
                .map(|t| t.title.as_str())
                .or_else(|| {
                    ["trackId", "sourceTrackId", "remoteSourceId"]
                        .into_iter()
                        .find_map(|key| source[key].as_str())
                })
                .unwrap_or(tr("曲目"));
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 60.).max(40.);
                    ui.add_sized(
                        [width, 22.],
                        egui::Label::new(title).halign(egui::Align::Min).truncate(),
                    )
                    .on_hover_text(title);
                    if ui.small_button(tr("移除")).clicked() {
                        removed = Some(index);
                    }
                });
            });
            if source["trackId"].is_string() {
                if let Some(group) = track.and_then(|t| t.sources.as_ref()) {
                    let current = source["forcedSourceTrackId"].as_str();
                    let label = current
                        .map(|id| {
                            group
                                .choices()
                                .find(|v| v.source_track_id == id)
                                .map_or(id, |v| v.display_name.as_str())
                        })
                        .unwrap_or(tr("跟随曲目默认来源"));
                    let mut pin = current.map(str::to_owned);
                    egui::ComboBox::from_id_salt(("entry-source", index))
                        .width(270.)
                        .truncate()
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut pin, None, tr("跟随曲目默认来源"));
                            for version in group.choices() {
                                ui.selectable_value(
                                    &mut pin,
                                    Some(version.source_track_id.clone()),
                                    &version.display_name,
                                );
                            }
                        });
                    if pin.as_deref() != current {
                        source["forcedSourceTrackId"] = json!(pin);
                        changed = true;
                    }
                }
            }
            ui.separator();
        }
        if let Some(index) = removed {
            entries.remove(index);
            if index < self.tracks.len() {
                self.tracks.remove(index);
            } else {
                // 正在读取的这一批发生了变化，丢弃旧响应，避免元数据错配。
                self.pending = None;
                self.error = None;
            }
            changed = true;
        }
        if self.pending.is_some() {
            ui.spinner();
        } else if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button(tr("重试")).clicked() {
                self.load(ui, svc, entries);
            }
        } else if self.tracks.len() < entries.len() && ui.button(tr("加载更多")).clicked() {
            self.load(ui, svc, entries);
        }
        changed
    }
}
