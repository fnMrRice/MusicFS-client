use crate::task::Task;
use eframe::egui;
use musicfs_server_adapter::{Page, ServerSession, Track};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use tokio::runtime::Handle;

#[cfg(test)]
#[path = "../../../tests/native/favorites.rs"]
mod tests;

#[derive(Default)]
pub struct Favorites {
    ids: Option<HashSet<String>>,
    // 收藏列表允许在读取全量索引时取消收藏。保留期间成功的写入，防止旧响应覆盖。
    changes: HashMap<String, bool>,
    pending: Option<Task<HashSet<String>>>,
    error: Option<String>,
}

impl Favorites {
    pub fn sync(&mut self, ctx: &egui::Context, rt: &Handle, server: &ServerSession) {
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            match result {
                Ok(ids) => self.install(ids),
                Err(error) => self.error = Some(error),
            }
        }
        if self.ids.is_none() && self.pending.is_none() && self.error.is_none() {
            let server = server.clone();
            self.pending = Some(Task::spawn(rt, ctx, async move {
                collect_ids(|cursor| {
                    let server = &server;
                    async move { server.favorites(cursor.as_deref()).await }
                })
                .await
            }));
        }
    }

    fn install(&mut self, mut ids: HashSet<String>) {
        for (id, enabled) in self.changes.drain() {
            if enabled {
                ids.insert(id);
            } else {
                ids.remove(&id);
            }
        }
        self.ids = Some(ids);
    }

    pub fn record(&mut self, id: String, enabled: bool) {
        if let Some(ids) = &mut self.ids {
            if enabled {
                ids.insert(id);
            } else {
                ids.remove(&id);
            }
        } else {
            self.changes.insert(id, enabled);
        }
    }

    pub fn contains(&self, id: &str, listed_as_favorite: bool) -> Option<bool> {
        self.changes.get(id).copied().or_else(|| {
            if listed_as_favorite {
                Some(true)
            } else {
                self.ids.as_ref().map(|ids| ids.contains(id))
            }
        })
    }

    pub fn refresh(&mut self) {
        if self.pending.is_none() {
            self.ids = None;
            self.error = None;
        }
    }

    pub fn status(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("收藏状态加载失败 · {error}"),
            );
            if ui.button("重试加载收藏状态").clicked() {
                self.error = None;
            }
        } else if self.pending.is_some() {
            ui.weak("正在读取收藏状态…");
        }
    }
}

async fn collect_ids<F, Fut>(mut fetch: F) -> Result<HashSet<String>, String>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = Result<Page<Track>, String>>,
{
    let mut ids = HashSet::new();
    let mut seen = HashSet::new();
    let mut cursor = None;
    loop {
        let page = fetch(cursor.take()).await?;
        ids.extend(page.items.into_iter().map(|track| track.id));
        let Some(next) = page.next_cursor else {
            return Ok(ids);
        };
        if next.is_empty() || !seen.insert(next.clone()) {
            return Err("服务器收藏分页未向前推进。".into());
        }
        cursor = Some(next);
    }
}
