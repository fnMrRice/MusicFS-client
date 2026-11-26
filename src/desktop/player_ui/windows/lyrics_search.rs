use crate::{i18n::tr, task::Task};
use eframe::egui;
use musicfs_player_core::Presentation;
use musicfs_server_adapter::{lyrics::LyricsMatches, ServerSession};
use std::sync::Arc;
use tokio::runtime::Handle;
use uuid::Uuid;

pub(super) struct LyricsSearch {
    query: Option<Task<LyricsMatches>>,
    matches: Option<Arc<LyricsMatches>>,
    save: Option<Task<()>>,
    retry: Option<(usize, String)>,
    error: Option<String>,
}

impl LyricsSearch {
    pub(super) fn start(
        ui: &egui::Ui,
        runtime: &Handle,
        server: &ServerSession,
        source: &str,
        now: &Presentation,
    ) -> Self {
        let server = server.clone();
        let source = source.to_owned();
        let title = now.title.clone().unwrap_or_default();
        let artist = now.artists.join(", ");
        let duration = now.duration_ms;
        Self {
            query: Some(Task::spawn(runtime, ui.ctx(), async move {
                server
                    .search_lyrics(&source, &title, &artist, duration)
                    .await
            })),
            matches: None,
            save: None,
            retry: None,
            error: None,
        }
    }

    pub(super) fn saving(&self) -> bool {
        self.save.is_some()
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        runtime: &Handle,
        server: &ServerSession,
    ) -> bool {
        if let Some(result) = self.query.as_mut().and_then(Task::poll) {
            self.query = None;
            match result {
                Ok(matches) => self.matches = Some(Arc::new(matches)),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(result) = self.save.as_mut().and_then(Task::poll) {
            self.save = None;
            match result {
                Ok(()) => return true,
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.query.is_some() {
            ui.weak(tr("正在查找歌词…"));
        }
        let Some(matches) = &self.matches else {
            return false;
        };
        if matches.items().is_empty() {
            ui.weak(tr("没有找到匹配的歌词"));
            return false;
        }
        if self.saving() {
            ui.weak(tr("正在保存…"));
        }
        let mut selected = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (index, item) in matches.items().iter().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.saving(), egui::Button::new(tr("使用这份歌词")))
                            .clicked()
                        {
                            selected = Some(index);
                        }
                        let label = [&item.title, &item.artist, &item.album]
                            .into_iter()
                            .map(String::as_str)
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · ");
                        ui.add(egui::Label::new(label).truncate());
                    });
                    let excerpt: String = item.lyrics.chars().take(240).collect();
                    let preview = excerpt.lines().take(3).collect::<Vec<_>>().join("\n");
                    ui.weak(preview);
                    ui.separator();
                });
            }
        });
        if let Some(index) = selected {
            // 同一个候选的保存请求在重试时沿用幂等键。
            let key = self
                .retry
                .take()
                .filter(|(i, _)| *i == index)
                .map_or_else(|| Uuid::new_v4().to_string(), |(_, key)| key);
            self.retry = Some((index, key.clone()));
            let matches = matches.clone();
            let server = server.clone();
            self.error = None;
            self.save = Some(Task::spawn(runtime, ui.ctx(), async move {
                matches.apply(&server, index, &key).await
            }));
        }
        false
    }
}
