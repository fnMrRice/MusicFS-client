use super::{
    api::{get, path_id, text, Load},
    Services,
};
use crate::i18n::tr;
use eframe::egui;
use musicfs_player_core::Track;
use musicfs_server_adapter::TrackInfo;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    tracks: Vec<TrackInfo>,
    next_cursor: Option<String>,
    truncated: bool,
}
#[derive(Default)]
pub(super) struct Search {
    open: bool,
    draft: String,
    query: String,
    results: Vec<TrackInfo>,
    next: Option<String>,
    seen: HashSet<String>,
    page: Load<Page>,
    truncated: bool,
    resolved: Load<(Track, bool)>,
    notice: Option<String>,
}
impl Search {
    pub fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>, instance: &Value) {
        if ui.link(tr("打开链接或编号")).clicked() {
            self.open = !self.open;
        }
        if self.resolved.poll() {
            let (track, play) = self.resolved.value.take().unwrap();
            let result = if play {
                svc.player.set_queue(vec![track], 0)
            } else {
                svc.player.enqueue(track)
            };
            self.notice = Some(result.map_or_else(
                |e| e,
                |_| {
                    tr(if play {
                        "正在播放"
                    } else {
                        "已加入队列"
                    })
                    .into()
                },
            ));
        }
        if !self.open {
            return;
        }
        let supported = |name: &str| {
            instance["capabilities"].as_array().is_some_and(|a| {
                a.iter()
                    .any(|v| v["name"] == name && v["supported"] == true)
            })
        };
        if text(instance, "availability") != "available" {
            ui.weak(tr("来源暂时离线，请稍后重试。"));
            return;
        }
        if !supported("search.tracks") {
            ui.weak(tr("此来源尚不支持曲目搜索。"));
            return;
        }
        if text(instance, "platform") == "bilibili" {
            ui.weak(tr(
                "输入公开视频 BV/AV、au 编号或完整链接；暂不支持关键词搜索。",
            ));
        }
        let mut load = false;
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.draft)
                    .char_limit(1024)
                    .hint_text(tr("链接或编号")),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if !self.draft.trim().is_empty() && (ui.button(tr("搜索")).clicked() || enter) {
                self.query = self.draft.trim().into();
                self.results.clear();
                self.seen.clear();
                self.next = None;
                self.page = Default::default();
                self.truncated = false;
                load = true;
            }
        });
        if self.page.poll() {
            let page = self.page.value.take().unwrap();
            if page
                .next_cursor
                .as_ref()
                .is_some_and(|n| n.is_empty() || !self.seen.insert(n.clone()))
            {
                self.page.error = Some("Repeated search cursor.".into());
                self.next = None;
            } else {
                let mut existing: HashSet<_> =
                    self.results.iter().map(|h| h.external_id.clone()).collect();
                self.results.extend(
                    page.tracks
                        .into_iter()
                        .filter(|h| existing.insert(h.external_id.clone())),
                );
                self.next = page.next_cursor;
                self.truncated = page.truncated;
            }
        }
        load |= self.page.status(ui);
        let mut action = None;
        for hit in &self.results {
            ui.push_id(&hit.external_id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(&hit.title);
                    if !hit.artists.is_empty() {
                        ui.weak(hit.artists.join(" / "));
                    }
                    if let Some(album) = &hit.album {
                        ui.weak(album);
                    }
                    if let Some(ms) = hit.duration_ms {
                        ui.weak(format!("{}:{:02}", ms / 60_000, ms / 1000 % 60));
                    }
                    if supported("stream.resolve") {
                        ui.add_enabled_ui(!self.resolved.busy(), |ui| {
                            if ui.button(tr("播放")).clicked() {
                                action = Some((hit.clone(), true));
                            }
                            if ui.button(tr("加入队列")).clicked() {
                                action = Some((hit.clone(), false));
                            }
                        });
                    }
                })
            });
        }
        if !self.query.is_empty()
            && !self.page.busy()
            && self.page.error.is_none()
            && !load
            && self.results.is_empty()
        {
            ui.weak(tr("没有搜索结果"));
        }
        if self.next.is_some() && !self.page.busy() && ui.button(tr("加载更多")).clicked() {
            load = true;
        }
        if self.truncated {
            ui.weak(tr("来源只返回了部分结果。"));
        }
        let instance_id = text(instance, "instanceId");
        if load {
            let mut params = url::form_urlencoded::Serializer::new(String::new());
            params
                .append_pair("q", &self.query)
                .append_pair("limit", "30");
            if let Some(next) = &self.next {
                params.append_pair("cursor", next);
            }
            let path = format!(
                "/online-instances/{}/search?{}",
                path_id(instance_id),
                params.finish()
            );
            let s = svc.server.clone();
            self.page
                .start(ui, svc, async move { get(&s, &path).await });
        }
        if let Some((hit, play)) = action {
            let (s, id) = (svc.server.clone(), instance_id.to_owned());
            self.notice = None;
            self.resolved.start(ui, svc, async move {
                Ok((
                    crate::ui::resolve_online_queue_track(&s, &id, hit).await?,
                    play,
                ))
            });
        }
        if self.resolved.busy() {
            ui.spinner();
        }
        if let Some(e) = self.resolved.error.as_ref().or(self.notice.as_ref()) {
            ui.label(e);
        }
    }
}
