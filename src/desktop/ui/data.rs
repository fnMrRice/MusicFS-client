use crate::i18n::tr;
use crate::task::Task;
use eframe::egui::Context;
use musicfs_player_core::{Player, Track as QueueTrack};
use musicfs_server_adapter::{CategoryKind, Page, ServerSession, Track};
use std::collections::HashSet;
use std::future::Future;
use tokio::runtime::Handle;

#[derive(Clone, Copy)]
pub struct Services<'a> {
    pub runtime: &'a Handle,
    pub server: &'a ServerSession,
    pub player: &'a Player,
}

pub struct Pages<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
    pub error: Option<String>,
    pub started: bool,
    seen: HashSet<String>,
    pending: Option<Task<Page<T>>>,
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
        }
    }
}

impl<T: Send + 'static> Pages<T> {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
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
        self.pending = Some(Task::spawn(rt, ctx, future));
    }

    pub fn poll(&mut self) {
        let Some(result) = self.pending.as_mut().and_then(Task::poll) else {
            return;
        };
        self.pending = None;
        match result {
            Ok(page) => {
                if page
                    .next_cursor
                    .as_ref()
                    .is_some_and(|c| !self.seen.insert(c.clone()))
                {
                    self.error = Some(tr("服务器分页未向前推进，请刷新列表。").into());
                    self.next = None;
                    return;
                }
                self.items.extend(page.items);
                self.next = page.next_cursor;
            }
            Err(err) => self.error = Some(err),
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
}

impl CategoryTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => tr("全部曲目"),
            Self::Artist => tr("歌手"),
            Self::Album => tr("专辑"),
            Self::Genre => tr("流派"),
        }
    }
    pub fn kind(self) -> Option<CategoryKind> {
        match self {
            Self::All => None,
            Self::Artist => Some(CategoryKind::Artist),
            Self::Album => Some(CategoryKind::Album),
            Self::Genre => Some(CategoryKind::Genre),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum TrackQuery {
    Library(String),
    Favorites,
    History,
    Playlist { id: String, revision: u64 },
    Category { tab: CategoryTab, id: String },
}

impl TrackQuery {
    pub async fn fetch(
        &self,
        server: &ServerSession,
        after: Option<&str>,
    ) -> Result<Page<Track>, String> {
        match self {
            Self::Library(q) => server.tracks(q, after).await,
            Self::History => server.history(after).await,
            Self::Favorites => server.favorites(after).await,
            Self::Playlist { id, revision } => server.playlist_tracks(id, *revision, after).await,
            Self::Category { tab, id } => {
                server
                    .category_tracks(tab.kind().ok_or("Invalid category.")?, id, after)
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
    })
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
