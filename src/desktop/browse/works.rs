use super::{
    resource::{Pages, Resource},
    work_detail::Detail,
    QueueSelection,
};
use crate::i18n::tr;
use eframe::egui;
use musicfs_server_adapter::{ServerSession, Work, WorkHistoryEntry};
use tokio::runtime::Handle;

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
        rt: &Handle,
        session: &ServerSession,
        kind: &str,
    ) -> Option<QueueSelection> {
        if self.kind != kind {
            *self = Self {
                kind: kind.into(),
                ..Default::default()
            };
        }
        if let Some(detail) = &mut self.detail {
            if ui.button(tr("‹  返回作品列表")).clicked() {
                self.detail = None;
            } else {
                ui.add_space(12.0);
                return detail.show(ui, rt, session, kind);
            }
        }
        self.pages.poll();
        let search = ui
            .horizontal_wrapped(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.draft)
                        .hint_text(tr("搜索作品名、创作者或简介"))
                        .char_limit(256)
                        .desired_width(250.0),
                );
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let submit = ui.button(tr("搜索")).clicked();
                let refresh = ui.button(tr("刷新")).clicked();
                enter || submit || refresh
            })
            .inner;
        if search {
            self.query = self.draft.trim().to_owned();
            self.pages = Pages::default();
        }
        let reload = self.pages.request.status(ui, tr("正在读取作品…"));
        if !self.pages.request.started || reload {
            self.load(ui.ctx(), rt, session);
        }
        ui.add_space(12.0);
        if self.pages.items.is_empty()
            && !self.pages.request.busy()
            && self.pages.request.error.is_none()
        {
            ui.weak(if self.query.is_empty() {
                tr("此作品库暂时为空。")
            } else {
                tr("没有匹配的作品。")
            });
        }
        for work in &self.pages.items {
            ui.push_id(&work.work_id, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                    if ui
                        .link(egui::RichText::new(&work.title).strong().size(17.0))
                        .clicked()
                    {
                        self.detail = Some(Detail::new(work.work_id.clone()));
                    }
                    if !work.credits.is_empty() {
                        ui.weak(
                            work.credits
                                .iter()
                                .map(|c| c.name.as_str())
                                .collect::<Vec<_>>()
                                .join(" / "),
                        );
                    }
                    if !work.tags.is_empty() {
                        ui.small(work.tags.join(" · "));
                    }
                });
                ui.add_space(6.0);
            });
        }
        if self.pages.next.is_some()
            && !self.pages.request.busy()
            && self.pages.request.error.is_none()
            && ui.button(tr("加载更多作品")).clicked()
        {
            self.load(ui.ctx(), rt, session);
        }
        None
    }

    fn load(&mut self, ctx: &egui::Context, rt: &Handle, session: &ServerSession) {
        let (server, kind, query, next) = (
            session.clone(),
            self.kind.clone(),
            self.query.clone(),
            self.pages.next.clone(),
        );
        self.pages.request.start(ctx, rt, async move {
            server.works(&kind, &query, next.as_deref()).await
        });
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
        rt: &Handle,
        session: &ServerSession,
        kind: &str,
    ) -> Option<QueueSelection> {
        if self.kind != kind {
            *self = Self {
                kind: kind.into(),
                ..Default::default()
            };
        }
        if let Some(detail) = &mut self.detail {
            if ui.button(tr("‹  返回播放记录")).clicked() {
                self.detail = None;
            } else {
                ui.add_space(10.0);
                return detail.show(ui, rt, session, kind);
            }
        }
        self.data.poll();
        let refresh = ui
            .add_enabled(!self.data.busy(), egui::Button::new(tr("刷新播放记录")))
            .clicked();
        let retry = self.data.status(ui, tr("正在读取播放记录…"));
        if !self.data.started || refresh || retry {
            let (server, kind) = (session.clone(), self.kind.clone());
            self.data.start(
                ui.ctx(),
                rt,
                async move { server.work_history(&kind).await },
            );
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
