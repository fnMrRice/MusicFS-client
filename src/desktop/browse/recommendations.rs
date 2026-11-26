mod cards;
mod detail;
use super::{resource::Resource, QueueSelection};
use crate::{
    i18n::tr,
    ui::{Artwork, Services},
};
use detail::Detail;
use eframe::egui;
use musicfs_server_adapter::RecommendationFeed;

#[derive(Default)]
pub(super) struct Recommendations {
    feed: Resource<RecommendationFeed>,
    requested: (u8, bool),
    offset: u8,
    selected: Option<Detail>,
}

impl Recommendations {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        artwork: &mut Artwork,
    ) -> Option<QueueSelection> {
        artwork.poll(ui.ctx());
        let scroll = egui::ScrollArea::vertical()
            .id_salt(("recommendations", self.selected.as_ref().map(Detail::key)));
        scroll
            .show(ui, |ui| {
                if let Some(detail) = &mut self.selected {
                    if ui.button(tr("‹  返回推荐")).clicked() {
                        self.selected = None;
                        ui.ctx().request_repaint();
                        return None;
                    }
                    ui.add_space(12.);
                    return detail.show(ui, svc, artwork);
                }
                self.feed(ui, svc, artwork);
                None
            })
            .inner
    }

    fn feed(&mut self, ui: &mut egui::Ui, svc: Services<'_>, artwork: &mut Artwork) {
        if self.feed.poll() {
            self.offset = self.requested.0;
        }
        let mut request = None;
        let depth = self.feed.value.as_ref().map_or(0, |f| f.history_depth);
        ui.horizontal_wrapped(|ui| {
            if self.offset > 0 {
                ui.weak(
                    tr("历史推荐 {0} / {1}")
                        .replace("{0}", &self.offset.to_string())
                        .replace("{1}", &depth.to_string()),
                );
                if ui
                    .add_enabled(!self.feed.busy(), egui::Button::new(tr("返回最新推荐")))
                    .clicked()
                {
                    request = Some((0, false));
                }
            }
            if depth > 0
                && ui
                    .add_enabled(
                        !self.feed.busy() && self.offset < depth,
                        egui::Button::new(tr("上一期")),
                    )
                    .clicked()
            {
                request = Some((self.offset + 1, false));
            }
            if ui
                .add_enabled(!self.feed.busy(), egui::Button::new(tr("刷新推荐")))
                .clicked()
            {
                request = Some((0, true));
            }
        });
        let retry = self.feed.status(ui, tr("正在读取推荐…"));
        if !self.feed.started || retry {
            request = Some(self.requested);
        }
        if let Some((history, refresh)) = request {
            let server = svc.server.clone();
            self.requested = (history, refresh);
            self.feed.start(ui.ctx(), svc.runtime, async move {
                server.recommendations(history, refresh).await
            });
        }
        let Some(feed) = &self.feed.value else {
            return;
        };
        ui.add_space(12.0);
        if feed.sources.is_empty() {
            ui.heading(tr("暂无推荐来源"));
            ui.weak(tr("连接的音乐库尚未提供推荐歌单。"));
        }
        for source in &feed.sources {
            ui.push_id(&source.instance_id, |ui| {
                let name = source
                    .label(crate::i18n::language_tags())
                    .unwrap_or_else(|| tr("未知来源").into());
                ui.heading(&name);
                if source.stale {
                    let date = source
                        .last_success_at
                        .as_deref()
                        .and_then(|s| s.get(..10))
                        .unwrap_or("");
                    ui.weak(tr("显示上次成功获取的推荐 {0}").replace("{0}", date));
                }
                if let Some(error) = &source.error {
                    let message = if error.code == "PLATFORM_AUTH_REQUIRED" {
                        tr("此来源需要登录平台账号。").to_owned()
                    } else {
                        tr("此来源暂时不可用（{0}）。").replace("{0}", &error.code)
                    };
                    ui.colored_label(ui.visuals().warn_fg_color, message);
                } else if source.items.is_empty() {
                    ui.weak(tr("此来源暂无推荐歌单。"));
                }
                if let Some(detail) = cards::show(ui, source, |ui, rect, cover| {
                    artwork.paint(ui, rect, svc, cover)
                }) {
                    self.selected = Some(detail);
                    ui.ctx().request_repaint();
                }
                ui.add_space(12.0);
            });
        }
    }
}
