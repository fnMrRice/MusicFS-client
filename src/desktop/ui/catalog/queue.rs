use super::super::actions::Actions;
use super::*;
use std::collections::HashSet;

impl Catalog {
    pub(super) fn poll_queue(&mut self, svc: Services<'_>, actions: &mut Actions) {
        if let Some((append, result)) = self
            .complete
            .as_mut()
            .and_then(|(append, t)| t.poll().map(|r| (*append, r)))
        {
            self.complete = None;
            match result.and_then(|q| apply_queue(svc, q, append)) {
                Ok(message) => self.feedback = message,
                Err(e) => actions.notice = Some((e, true)),
            }
        }
    }
    pub(super) fn queue_buttons(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        actions: &mut Actions,
    ) {
        let ready = self.complete.is_none()
            && (!self.playlist.tools.active() || self.tracks.next.is_none())
            && !self.tracks.busy()
            && self.tracks.error.is_none()
            && (self.tracks.next.is_some()
                || self
                    .tracks
                    .items
                    .iter()
                    .any(|t| t.playable_source_id().is_some()));
        let play = style::primary(ui, tr("播放全部"), ready).clicked();
        let append = ui
            .add_enabled(ready, egui::Button::new(tr("加入队列")))
            .clicked();
        if !play && !append {
            return;
        }
        let Some(query) = self.query.clone() else {
            return;
        };
        let mut q: Vec<_> = self.tracks.items.iter().filter_map(queue_track).collect();
        let mut cursor = self.tracks.next.clone();
        if cursor.is_none() {
            match apply_queue(svc, q, append) {
                Ok(message) => self.feedback = message,
                Err(e) => actions.notice = Some((e, true)),
            }
        } else {
            let server = svc.server.clone();
            self.complete = Some((
                append,
                Task::spawn(svc.runtime, ui.ctx(), async move {
                    let mut seen = HashSet::new();
                    while let Some(after) = cursor.take() {
                        if !seen.insert(after.clone()) {
                            return Err(tr("服务器分页未向前推进。").into());
                        }
                        let page = query.fetch(&server, Some(&after)).await?;
                        q.extend(page.items.iter().filter_map(queue_track));
                        cursor = page.next_cursor;
                    }
                    Ok(q)
                }),
            ));
        }
    }
}

pub(super) fn enqueue(svc: Services<'_>, q: Vec<QueueTrack>) -> Result<String, String> {
    let count = q.len();
    svc.player.enqueue_many(q)?;
    Ok(tr("已添加 {0} 首到队列").replace("{0}", &count.to_string()))
}
fn apply_queue(
    svc: Services<'_>,
    q: Vec<QueueTrack>,
    append: bool,
) -> Result<Option<String>, String> {
    if q.is_empty() {
        return Err(tr("列表中没有可播放的音源。").into());
    }
    if append {
        enqueue(svc, q).map(Some)
    } else {
        svc.player.set_queue(q, 0).map(|_| None)
    }
}
