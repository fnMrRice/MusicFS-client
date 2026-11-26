use crate::i18n::tr;
use crate::task::Task;
use eframe::egui::Context;
use musicfs_player_core::{Player, Track as QueueTrack};
use musicfs_server_adapter::{
    CategoryKind, Page, ResolvedOnlineTrack, ServerSession, Track, TrackInfo,
};
use std::collections::HashSet;
use std::future::Future;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

#[cfg(test)]
#[path = "../../../tests/native/online_queue_track.rs"]
mod online_queue_tests;

#[cfg(test)]
#[path = "../../../tests/native/catalog_paging.rs"]
mod catalog_paging;

#[derive(Clone, Copy)]
pub struct Services<'a> {
    pub runtime: &'a Handle,
    pub activity: &'a musicfs_server_adapter::activity::ActivityStream,
    pub server: &'a ServerSession,
    pub player: &'a Player,
}

struct FirstPage {
    requested_at: Instant,
    count: usize,
    next: Option<String>,
}

pub struct Pages<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
    pub error: Option<String>,
    pub started: bool,
    seen: HashSet<String>,
    pending: Option<(Instant, Task<Page<T>>)>,
    first_page: Option<FirstPage>,
    replace_on_success: bool,
}

impl<T> Default for Pages<T> {
    fn default() -> Self {
        Self {
            items: vec![],
            next: None,
            error: None,
            started: false,
            seen: HashSet::new(),
            pending: None,
            first_page: None,
            replace_on_success: false,
        }
    }
}

impl<T: Send + 'static> Pages<T> {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn can_load_more(&self) -> bool {
        self.next.is_some() && !self.busy() && self.error.is_none()
    }

    pub fn load(
        &mut self,
        ctx: &Context,
        rt: &Handle,
        future: impl Future<Output = Result<Page<T>, String>> + Send + 'static,
    ) {
        if self.busy() {
            return;
        }
        self.started = true;
        self.error = None;
        self.pending = Some((Instant::now(), Task::spawn(rt, ctx, future)));
    }

    pub fn poll(&mut self) -> bool {
        let replacing = self.replace_on_success;
        if let Some(items) = self.take_page() {
            self.items.extend(items);
            return replacing;
        }
        false
    }

    pub(super) fn request_cursor(&self) -> Option<String> {
        if self.replace_on_success {
            None
        } else {
            self.next.clone()
        }
    }

    pub(super) fn resume(&mut self, max_age: Duration) {
        self.poll();
        if !self.replace_on_success
            && self
                .first_page
                .as_ref()
                .is_some_and(|page| page.requested_at.elapsed() >= max_age)
        {
            // 过期游标的续页不能并入新首页。
            self.pending = None;
            self.replace_on_success = true;
            self.started = false;
        }
        if self.error.take().is_some() {
            self.started = false;
        }
    }

    pub(super) fn can_cache(&self) -> bool {
        self.first_page.is_some() || self.busy()
    }

    // 大列表离开视图时仅留首页及其对应的游标，仍可从正确的位置继续翻页。
    pub(super) fn limit_cached_rows(&mut self, limit: usize) {
        if self.items.len() <= limit {
            return;
        }
        if let Some(page) = &self.first_page {
            let mut rows = std::mem::take(&mut self.items).into_iter();
            self.items.extend(rows.by_ref().take(page.count));
            // 深翻曲库后，逐项释放字符串和音源会拖慢切换帧。
            let _ = std::thread::Builder::new()
                .name("catalog-cache-cleanup".into())
                .spawn(move || drop(rows));
            self.next = page.next.clone();
            self.seen = page.next.iter().cloned().collect();
            if !self.replace_on_success {
                self.pending = None;
            }
        }
    }

    pub(super) fn take_page(&mut self) -> Option<Vec<T>> {
        let (requested, task) = self.pending.as_mut()?;
        let requested = *requested;
        let result = task.poll()?;
        self.pending = None;
        match result {
            Ok(page) => {
                if page.next_cursor.as_ref().is_some_and(|c| {
                    c.is_empty() || (!self.replace_on_success && self.seen.contains(c))
                }) {
                    self.error = Some(tr("服务器分页未向前推进，请刷新列表。").into());
                    return None;
                }
                if self.replace_on_success {
                    self.items.clear();
                    self.seen.clear();
                    self.first_page = None;
                    self.replace_on_success = false;
                }
                if self.first_page.is_none() {
                    self.first_page = Some(FirstPage {
                        requested_at: requested,
                        count: page.items.len(),
                        next: page.next_cursor.clone(),
                    });
                }
                if let Some(cursor) = &page.next_cursor {
                    self.seen.insert(cursor.clone());
                }
                self.next = page.next_cursor;
                Some(page.items)
            }
            Err(err) => {
                self.error = Some(err);
                None
            }
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum CategoryTab {
    #[default]
    All,
    Artist,
    Album,
    Genre,
    Folder,
}

impl CategoryTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => tr("全部曲目"),
            Self::Artist => tr("歌手"),
            Self::Album => tr("专辑"),
            Self::Genre => tr("流派"),
            Self::Folder => tr("文件夹"),
        }
    }
    pub fn kind(self) -> Option<CategoryKind> {
        match self {
            Self::All | Self::Folder => None,
            Self::Artist => Some(CategoryKind::Artist),
            Self::Album => Some(CategoryKind::Album),
            Self::Genre => Some(CategoryKind::Genre),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum TrackQuery {
    Library(String),
    AvailableSources {
        query: String,
    },
    Favorites,
    History,
    Playlist {
        id: String,
        revision: u64,
    },
    Category {
        tab: CategoryTab,
        id: String,
        query: String,
    },
}

impl TrackQuery {
    pub fn is_playlist(&self) -> bool {
        matches!(self, Self::Favorites | Self::Playlist { .. } | Self::History)
    }

    pub async fn fetch(
        &self,
        server: &ServerSession,
        after: Option<&str>,
    ) -> Result<Page<Track>, String> {
        match self {
            Self::Library(q) => server.tracks(q, after).await,
            Self::AvailableSources { query } => {
                server.search_sourced_tracks(None, query, after).await
            }
            Self::History => server.history(after).await,
            Self::Favorites => server.favorites(after).await,
            Self::Playlist { id, revision } => server.playlist_tracks(id, *revision, after).await,
            Self::Category { tab, id, query } => {
                server
                    .category_tracks(
                        tab.kind().ok_or("Invalid category.")?,
                        id,
                        None,
                        query,
                        after,
                    )
                    .await
            }
        }
    }
}

pub fn queue_track(track: &Track) -> Option<QueueTrack> {
    track.playable_source_id()?;
    Some(QueueTrack {
        id: track.id.clone(),
        source_only: false,
        title: track.title.clone(),
        artists: track.artists.clone(),
        album: track.album.clone(),
        source_track_id: track.playable_source_id().map(str::to_owned),
        forced_source_track_id: track.forced_source_track_id.clone(),
        duration_ms: track.source().and_then(|s| s.duration_ms),
        cover_object_id: track.cover().map(str::to_owned),
        online_cover_path: track.online_cover().map(str::to_owned),
    })
}

pub(crate) async fn resolve_online_queue_track(
    server: &ServerSession,
    instance: &str,
    entry: TrackInfo,
) -> Result<QueueTrack, String> {
    let source = server
        .resolve_online_track(instance, &entry.external_id)
        .await?;
    Ok(online_queue_track(source, entry))
}

fn online_queue_track(source: ResolvedOnlineTrack, entry: TrackInfo) -> QueueTrack {
    QueueTrack {
        id: source.source_track_id.clone(),
        source_only: true,
        title: source.title.unwrap_or(entry.title),
        artists: source.artists.unwrap_or(entry.artists),
        album: source.album.or(entry.album),
        source_track_id: Some(source.source_track_id),
        forced_source_track_id: None,
        duration_ms: source.duration_ms.or(entry.duration_ms),
        cover_object_id: source.cover_object_id,
        online_cover_path: source.online_cover_path.or(entry.cover_url),
    }
}

pub fn play_row(player: &Player, rows: &[Track], index: usize) -> Result<(), String> {
    play_row_version(player, rows, index, None)
}
pub fn play_row_version(
    player: &Player,
    rows: &[Track],
    index: usize,
    explicit: Option<String>,
) -> Result<(), String> {
    let mut selected = rows.get(index).ok_or("Invalid track row.")?.clone();
    if explicit.is_some() {
        selected.forced_source_track_id = explicit;
    }
    let item = queue_track(&selected).ok_or(tr("此曲目暂无可用音源。"))?;
    let mut queue: Vec<_> = rows[..index].iter().filter_map(queue_track).collect();
    let start = queue.len();
    queue.push(item);
    queue.extend(rows[index + 1..].iter().filter_map(queue_track));
    player.set_queue(queue, start).map(|_| ())
}
