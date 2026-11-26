use super::{Actions, Services};
use crate::{i18n::tr, task::Task, ui::favorites::favorite_button};
use eframe::egui;
use musicfs_player_core::Track;

#[derive(Default)]
pub(super) struct PlayingSource {
    source: Option<String>,
    pending: Option<Task<String>>,
    result: Option<Result<String, String>>,
}

impl PlayingSource {
    fn track_id<'a>(
        &'a mut self,
        ctx: &egui::Context,
        svc: Services<'_>,
        track: &'a Track,
    ) -> Result<Option<&'a str>, &'a str> {
        let source = track
            .source_track_id
            .as_deref()
            .filter(|_| track.source_only);
        if self.source.as_deref() != source {
            self.source = source.map(str::to_owned);
            self.result = None;
            // 切歌时替换接收端，上一首来源的返回值不能影响当前收藏按钮。
            self.pending = source.map(|id| {
                let server = svc.server.clone();
                let id = id.to_owned();
                Task::spawn(svc.runtime, ctx, async move {
                    server.track_id_for_source(&id).await
                })
            });
        }
        if !track.source_only {
            return Ok(Some(&track.id));
        }
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            self.result = Some(result);
        }
        self.result
            .as_ref()
            .map(|r| r.as_deref().map_err(String::as_str))
            .transpose()
    }
}

impl Actions {
    pub fn player_favorite(&mut self, ui: &mut egui::Ui, svc: Services<'_>, track: &Track) {
        let busy = self.busy();
        let id = match self.playing_source.track_id(ui.ctx(), svc, track) {
            Ok(id) => id,
            Err(error) => {
                if favorite_button(ui, false, tr("重试加载收藏状态"), 24.)
                    .on_hover_text(error)
                    .clicked()
                {
                    self.playing_source = Default::default();
                }
                return;
            }
        };
        let state = id.and_then(|id| self.favorites.contains(id, false));
        let saved = state == Some(true);
        let label = tr(if saved {
            "取消收藏"
        } else {
            "加入收藏"
        });
        if ui
            .add_enabled_ui(!busy && state.is_some(), |ui| {
                favorite_button(ui, saved, label, 24.)
            })
            .inner
            .clicked()
        {
            if let Some(id) = id.map(str::to_owned) {
                self.toggle(ui, svc, id, !saved);
            }
        }
    }
}
