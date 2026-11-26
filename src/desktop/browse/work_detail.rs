use super::{resource::Resource, work_gallery as gallery, QueueSelection};
use crate::{
    i18n::tr,
    ui::{style, Artwork, Services},
};
use eframe::egui::{self, FontId, RichText};
use musicfs_player_core::Track;
use musicfs_server_adapter::{AudioNode, PackageAudio, Work, WorkDetail, WorkType};
use std::collections::HashMap;

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

    pub fn id(&self) -> &str {
        &self.id
    }

    fn load(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let (server, id) = (svc.server.clone(), self.id.clone());
        self.data.start(ui.ctx(), svc.runtime, async move {
            server.work_detail(&id).await
        });
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        artwork: &mut Artwork,
        kind: &WorkType,
    ) -> Option<QueueSelection> {
        self.data.poll();
        if !self.data.started {
            self.load(ui, svc);
        }
        if let Some(error) = &self.data.error {
            if gallery::message(
                ui,
                "server",
                tr("作品资料读取失败"),
                error,
                Some(tr("重试")),
            ) {
                self.load(ui, svc);
            }
            return None;
        }
        let Some(data) = self.data.value.as_ref() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(tr("正在读取作品…"));
            });
            return None;
        };
        if data.work.content_type != kind.id {
            gallery::message(
                ui,
                gallery::type_icon(kind),
                tr("此作品未收录在当前作品库中。"),
                "",
                None,
            );
            return None;
        }
        artwork.poll(ui.ctx());
        self.copy = self.copy.min(data.packages.len().saturating_sub(1));
        let cover = data.packages.get(self.copy).and_then(|pack| {
            pack.audio
                .iter()
                .find_map(|node| node.cover_object_id.as_deref())
        });
        summary(ui, &data.work, kind, |ui, rect| {
            if cover.is_some() {
                artwork.paint(ui, rect, svc, cover);
            } else {
                gallery::mark(ui, rect, gallery::type_icon(kind));
            }
        });
        ui.add_space(12.);
        if data.packages.is_empty() {
            gallery::message(ui, "folder", tr("此作品尚未关联本地副本。"), "", None);
            return None;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("本地副本")).font(style::bold(ui, 18.)));
            egui::ComboBox::from_id_salt(("work-copy", &self.id))
                .selected_text(&data.packages[self.copy].name)
                .width(210.)
                .show_ui(ui, |ui| {
                    for (index, copy) in data.packages.iter().enumerate() {
                        ui.selectable_value(&mut self.copy, index, &copy.name);
                    }
                });
        });
        let copy = &data.packages[self.copy];
        ui.add_space(6.);
        ui.push_id(&copy.id, |ui| show_audio(ui, copy, svc, artwork))
            .inner
    }
}

fn summary(
    ui: &mut egui::Ui,
    work: &Work,
    kind: &WorkType,
    cover: impl FnOnce(&egui::Ui, egui::Rect),
) {
    gallery::panel(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(76.), egui::Sense::hover());
            cover(ui, rect);
            ui.add_space(4.);
            ui.vertical(|ui| {
                ui.label(RichText::new(&work.title).font(style::bold(ui, 23.)));
                if !work.external_ids.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for id in &work.external_ids {
                            ui.weak(format!(
                                "{}  {}",
                                tr(kind.namespace_label(&id.namespace)),
                                id.value
                            ));
                        }
                    });
                }
                if !work.credits.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for credit in &work.credits {
                            ui.label(format!(
                                "{}  {}",
                                tr(kind.role_label(&credit.role)),
                                credit.name
                            ));
                        }
                    });
                }
            });
        });
        if !work.tags.is_empty() || !work.description.is_empty() {
            ui.add_space(5.);
            ui.separator();
            ui.add_space(4.);
            if !work.description.is_empty() {
                ui.label(&work.description);
            }
            if !work.tags.is_empty() {
                ui.colored_label(style::accent(ui), work.tags.join("   ·   "));
            }
        }
    });
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
            online_cover_path: None,
        });
    }
    if tracks.is_empty() {
        return None;
    }
    Some((tracks, if selected.is_some() { index? } else { 0 }))
}

fn show_audio(
    ui: &mut egui::Ui,
    pack: &PackageAudio,
    svc: Services<'_>,
    artwork: &mut Artwork,
) -> Option<QueueSelection> {
    if pack.audio.is_empty() {
        gallery::message(ui, "music", tr("此副本暂无音轨。"), "", None);
        return None;
    }
    // 目录按清单顺序显示，目录内按服务端定义的音轨序号播放。
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
        let directory = &nodes[0].directory;
        let name = if directory.is_empty() {
            &pack.name
        } else {
            directory
        };
        ui.push_id(&nodes[0].group, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).font(style::bold(ui, 15.)));
                ui.weak(tr("{0} 条音轨").replace("{0}", &nodes.len().to_string()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            nodes.iter().any(|n| n.source_track_id.is_some()),
                            egui::Button::new(tr("播放此目录")),
                        )
                        .clicked()
                    {
                        choice = group_queue(nodes, None);
                    }
                });
            });
            for (index, node) in nodes.iter().enumerate() {
                if ui
                    .push_id(&node.id, |ui| track_row(ui, svc, artwork, node, index + 1))
                    .inner
                {
                    choice = group_queue(nodes, Some(&node.id));
                }
            }
            ui.add_space(8.);
        });
    }
    choice
}

fn track_row(
    ui: &mut egui::Ui,
    svc: Services<'_>,
    artwork: &mut Artwork,
    node: &AudioNode,
    index: usize,
) -> bool {
    let can_play = node.source_track_id.is_some();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 54.),
        if can_play {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, can_play, &node.title));
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let dark = ui.visuals().dark_mode;
    let muted = style::muted(dark);
    let fill = if can_play {
        if response.hovered() || response.has_focus() {
            style::hovered(dark)
        } else {
            style::surface(dark)
        }
    } else {
        style::surface(dark)
    };
    ui.painter().rect_filled(rect, 6, fill);
    ui.painter().text(
        egui::pos2(rect.left() + 18., rect.center().y),
        egui::Align2::CENTER_CENTER,
        index.to_string(),
        FontId::proportional(13.),
        muted,
    );
    let cover = egui::Rect::from_min_size(rect.min + egui::vec2(38., 7.), egui::Vec2::splat(40.));
    artwork.paint(ui, cover, svc, node.cover_object_id.as_deref());
    let right = rect.right() - 86.;
    let title_y = if node.artists.is_empty() {
        rect.center().y - 10.
    } else {
        rect.top() + 6.
    };
    let text_area = |y| {
        egui::Rect::from_min_max(
            egui::pos2(cover.right() + 12., y),
            egui::pos2(right, rect.bottom()),
        )
    };
    gallery::lines(
        ui,
        text_area(title_y),
        &node.title,
        style::bold(ui, 15.),
        if can_play { style::text(dark) } else { muted },
        1,
    );
    if !node.artists.is_empty() {
        gallery::lines(
            ui,
            text_area(rect.top() + 29.),
            &node.artists.join(" / "),
            FontId::proportional(12.),
            muted,
            1,
        );
    }
    let end = if can_play {
        duration(node.duration_ms)
    } else {
        tr("不可播放").into()
    };
    ui.painter().text(
        egui::pos2(rect.right() - 12., rect.center().y),
        egui::Align2::RIGHT_CENTER,
        end,
        FontId::proportional(13.),
        muted,
    );
    can_play
        && response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(tr("从此曲目播放"))
            .clicked()
}
