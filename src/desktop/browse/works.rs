use super::{
    resource::{Pages, Resource},
    work_detail::Detail,
    work_gallery as gallery, QueueSelection,
};
use crate::{
    i18n::tr,
    icons,
    ui::{style, Artwork, Services},
};
use eframe::egui;
use musicfs_server_adapter::{Work, WorkHistoryEntry, WorkType};

#[derive(Default)]
pub(super) struct Works {
    kind: String,
    draft: String,
    query: String,
    pages: Pages<Work>,
    detail: Option<Detail>,
}

impl Works {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        artwork: &mut Artwork,
        kind: &WorkType,
    ) -> Option<QueueSelection> {
        if self.kind != kind.id {
            *self = Self {
                kind: kind.id.clone(),
                ..Default::default()
            };
        }
        if let Some(detail) = &mut self.detail {
            if ui.button(tr("‹  返回作品列表")).clicked() {
                self.detail = None;
            } else {
                ui.add_space(6.);
                return egui::ScrollArea::vertical()
                    .id_salt(("work-detail", detail.id()))
                    .show(ui, |ui| detail.show(ui, svc, artwork, kind))
                    .inner;
            }
        }
        self.pages.poll();
        let mut reload = false;
        ui.horizontal(|ui| {
            let edit = ui.add_sized(
                [ui.available_width().min(420.), 38.],
                egui::TextEdit::singleline(&mut self.draft)
                    .hint_text(tr("搜索作品名、创作者或简介"))
                    .char_limit(256)
                    .margin(egui::Margin::symmetric(12, 6))
                    .vertical_align(egui::Align::Center),
            );
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                edit.request_focus();
            }
            let submit = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if style::primary(ui, tr("搜索"), true).clicked() || submit {
                self.query = self.draft.trim().to_owned();
                reload = true;
            }
            if !self.draft.is_empty() || !self.query.is_empty() {
                if icons::button(ui, "close", tr("清除搜索"), 34., 15.).clicked() {
                    self.draft.clear();
                    self.query.clear();
                    reload = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, "refresh", tr("刷新列表"), 38., 19.).clicked() {
                    reload = true;
                }
            });
        });
        ui.add_space(5.);
        ui.separator();
        ui.add_space(7.);
        if reload {
            self.pages = Pages::default();
        }
        if !self.pages.request.started {
            self.load(ui, svc);
        }
        egui::ScrollArea::vertical()
            .id_salt(("work-list", &kind.id, &self.query))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if let Some(id) = gallery::cards(ui, &self.pages.items, kind) {
                    self.detail = Some(Detail::new(id));
                    ui.ctx().request_repaint();
                }
                if let Some(error) = &self.pages.request.error {
                    if gallery::message(
                        ui,
                        "server",
                        tr("作品列表读取失败"),
                        error,
                        Some(tr("重试")),
                    ) {
                        self.load(ui, svc);
                    }
                } else if self.pages.request.busy() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.weak(tr("正在读取作品…"));
                    });
                } else if self.pages.next.is_some() {
                    if crate::ui::page_end_visible(ui) {
                        self.load(ui, svc);
                    }
                } else if self.pages.items.is_empty() {
                    if self.query.is_empty() {
                        empty_library(ui, svc, kind);
                    } else {
                        let note = tr("没有与“{0}”匹配的作品。").replace("{0}", &self.query);
                        if gallery::message(
                            ui,
                            "search",
                            tr("未找到作品"),
                            &note,
                            Some(tr("清除搜索")),
                        ) {
                            self.query.clear();
                            self.draft.clear();
                            self.pages = Pages::default();
                            ui.ctx().request_repaint();
                        }
                    }
                }
            });
        None
    }
    fn load(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let (server, kind, query, next) = (
            svc.server.clone(),
            self.kind.clone(),
            self.query.clone(),
            self.pages.next.clone(),
        );
        self.pages.request.start(ui.ctx(), svc.runtime, async move {
            server.works(&kind, &query, next.as_deref()).await
        });
    }
}

fn empty_library(ui: &mut egui::Ui, svc: Services<'_>, kind: &WorkType) {
    let (title, note) = match kind.id.as_str() {
        "cd_album" => (
            "尚未收录专辑",
            "在网页作品库中添加专辑并关联本地副本，即可在这里浏览和播放音轨。",
        ),
        "asmr" => (
            "尚未收录音声作品",
            "在网页作品库中添加音声作品并关联本地副本，即可在这里按目录收听。",
        ),
        "radio" => (
            "尚未收录电台节目",
            "在网页作品库中添加节目并关联本地录音，即可在这里收听。",
        ),
        _ => (
            "此作品库暂时为空。",
            "在网页作品库中添加作品并关联本地副本，即可在这里播放。",
        ),
    };
    if gallery::message(
        ui,
        gallery::type_icon(kind),
        tr(title),
        tr(note),
        Some(tr("打开网页作品库")),
    ) {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair("contentType", &kind.id);
        ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
            "{}/#/works?{}",
            svc.server.info.endpoint,
            query.finish()
        )));
    }
}

#[derive(Default)]
pub(super) struct History {
    kind: String,
    data: Resource<Vec<WorkHistoryEntry>>,
    detail: Option<Detail>,
}

impl History {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        artwork: &mut Artwork,
        kind: &WorkType,
    ) -> Option<QueueSelection> {
        if self.kind != kind.id {
            *self = Self {
                kind: kind.id.clone(),
                ..Default::default()
            };
        }
        if let Some(detail) = &mut self.detail {
            if ui.button(tr("‹  返回播放记录")).clicked() {
                self.detail = None;
            } else {
                ui.add_space(10.0);
                return detail.show(ui, svc, artwork, kind);
            }
        }
        self.data.poll();
        let refresh = ui
            .add_enabled(!self.data.busy(), egui::Button::new(tr("刷新播放记录")))
            .clicked();
        let retry = self.data.status(ui, tr("正在读取播放记录…"));
        if !self.data.started || refresh || retry {
            let (server, kind) = (svc.server.clone(), self.kind.clone());
            self.data.start(ui.ctx(), svc.runtime, async move {
                server.work_history(&kind).await
            });
        }
        ui.add_space(10.0);
        if let Some(entries) = &self.data.value {
            if entries.is_empty() {
                ui.weak(tr("暂无播放记录。"));
            }
            for entry in entries {
                ui.horizontal_wrapped(|ui| {
                    if ui.link(&entry.work_title).clicked() {
                        self.detail = Some(Detail::new(entry.work_id.clone()));
                        ui.ctx().request_repaint();
                    }
                    ui.label(&entry.track_title);
                    ui.weak(&entry.recorded_at);
                });
                ui.separator();
            }
        }
        None
    }
}
