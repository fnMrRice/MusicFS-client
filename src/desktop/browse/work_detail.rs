use super::{resource::Resource, QueueSelection};
use eframe::egui;
use musicfs_player_core::Track;
use musicfs_server_adapter::{AudioNode, PackageAudio, ServerSession, WorkDetail};
use std::collections::HashMap;
use tokio::runtime::Handle;

pub(super) struct Detail {
    id: String,
    data: Resource<WorkDetail>,
    copy: usize,
}

impl Detail {
    pub fn new(id: String) -> Self {
        Self {
            id,
            data: Resource::default(),
            copy: 0,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        rt: &Handle,
        session: &ServerSession,
        type_id: &str,
    ) -> Option<QueueSelection> {
        self.data.poll();
        if !self.data.started || self.data.status(ui, "正在读取作品…") {
            let server = session.clone();
            let id = self.id.clone();
            self.data
                .start(ui.ctx(), rt, async move { server.work_detail(&id).await });
        }
        let data = self.data.value.as_ref()?;
        let work = &data.work;
        if work.content_type != type_id {
            ui.label("此作品未收录在当前作品库中。");
            return None;
        }
        ui.heading(&work.title);
        if !work.credits.is_empty() {
            ui.label(
                work.credits
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" / "),
            );
        }
        if !work.tags.is_empty() {
            ui.weak(work.tags.join(" · "));
        }
        if !work.description.is_empty() {
            ui.add_space(6.0);
            ui.label(&work.description);
        }
        if data.packages.is_empty() {
            ui.add_space(12.0);
            ui.weak("此作品尚未关联本地副本。");
            return None;
        }
        ui.add_space(12.0);
        self.copy = self.copy.min(data.packages.len() - 1);
        ui.horizontal(|ui| {
            ui.label("本地副本");
            egui::ComboBox::from_id_salt(("work-copy", &self.id))
                .selected_text(&data.packages[self.copy].name)
                .show_ui(ui, |ui| {
                    for (index, copy) in data.packages.iter().enumerate() {
                        ui.selectable_value(&mut self.copy, index, &copy.name);
                    }
                });
        });
        let copy = &data.packages[self.copy];
        ui.add_space(10.0);
        ui.push_id(&copy.id, |ui| show_audio(ui, copy)).inner
    }
}

fn duration(ms: Option<u64>) -> String {
    ms.map(|v| format!("{}:{:02}", v / 60_000, v / 1000 % 60))
        .unwrap_or_else(|| "—".into())
}

fn group_queue(nodes: &[&AudioNode], selected: Option<&str>) -> Option<QueueSelection> {
    let mut tracks = Vec::new();
    let mut index = None;
    for node in nodes {
        let Some(source) = &node.source_track_id else {
            continue;
        };
        if selected == Some(node.id.as_str()) {
            index = Some(tracks.len());
        }
        tracks.push(Track {
            id: source.clone(),
            source_only: true,
            title: node.title.clone(),
            artists: node.artists.clone(),
            album: node.album.clone(),
            source_track_id: Some(source.clone()),
            forced_source_track_id: None,
            duration_ms: node.duration_ms,
            cover_object_id: node.cover_object_id.clone(),
        });
    }
    if tracks.is_empty() {
        return None;
    }
    Some((tracks, if selected.is_some() { index? } else { 0 }))
}

fn show_audio(ui: &mut egui::Ui, pack: &PackageAudio) -> Option<QueueSelection> {
    if pack.audio.is_empty() {
        ui.weak("此副本暂无音轨。");
        return None;
    }
    // 保持目录在清单中的顺序，目录内部按服务端音轨顺序排列。
    let mut groups: Vec<Vec<&AudioNode>> = Vec::new();
    let mut indexes = HashMap::new();
    for node in &pack.audio {
        let index = *indexes.entry(node.group.as_str()).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[index].push(node);
    }
    let mut choice = None;
    for nodes in &mut groups {
        nodes.sort_by_key(|n| n.order);
        let name = if nodes[0].directory.is_empty() {
            &pack.name
        } else {
            &nodes[0].directory
        };
        ui.push_id(&nodes[0].group, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(name);
                if ui
                    .add_enabled(
                        nodes.iter().any(|n| n.source_track_id.is_some()),
                        egui::Button::new("播放此目录"),
                    )
                    .clicked()
                {
                    choice = group_queue(nodes, None);
                }
            });
            ui.separator();
            for node in nodes.iter() {
                ui.push_id(&node.id, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let title = egui::Button::new(&node.title).frame(false);
                        if ui
                            .add_enabled(node.source_track_id.is_some(), title)
                            .on_hover_text("从此曲目播放")
                            .clicked()
                        {
                            choice = group_queue(nodes, Some(&node.id));
                        }
                        if !node.artists.is_empty() {
                            ui.weak(node.artists.join(" / "));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(if node.source_track_id.is_some() {
                                duration(node.duration_ms)
                            } else {
                                "不可播放".into()
                            });
                        });
                    });
                });
            }
            ui.add_space(12.0);
        });
    }
    choice
}
