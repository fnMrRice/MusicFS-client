mod actor;
mod decode;
mod pcm;

#[cfg(test)]
#[path = "../../tests/native/player.rs"]
mod tests;

use serde::{Deserialize, Serialize};
use std::io::{Read, Seek};
use std::sync::mpsc::{channel, Sender};
use std::thread;

pub const SUPPORTED_MIMES: &[&str] = &["audio/mpeg", "audio/flac", "audio/wav", "audio/x-wav"];

pub trait AudioReader: Read + Seek + Send + Sync + 'static {}
impl<T: Read + Seek + Send + Sync + 'static> AudioReader for T {}

pub struct Media {
    pub reader: Box<dyn AudioReader>,
    pub source_track_id: String,
    pub seek_range_ms: Option<(u64, u64)>,
    pub byte_length: Option<u64>,
    pub seekable: bool,
    pub on_started: Box<dyn FnOnce() + Send>,
    pub on_close: Box<dyn FnOnce() + Send>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    #[serde(default)]
    pub source_only: bool,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub source_track_id: Option<String>,
    pub forced_source_track_id: Option<String>,
    pub cover_object_id: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Idle,
    Loading,
    Playing,
    Paused,
    Error,
    Ended,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub queue: Vec<Track>,
    pub index: Option<usize>,
    pub status: Status,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub volume: f32,
    pub seekable: bool,
    pub seek_range_ms: Option<(u64, u64)>,
    pub active_source_track_id: Option<String>,
    pub error: Option<String>,
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            index: None,
            status: Status::Idle,
            position_ms: 0,
            duration_ms: None,
            volume: 1.0,
            seekable: false,
            seek_range_ms: None,
            active_source_track_id: None,
            error: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlayerCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    Clear,
    Seek { position_ms: u64 },
    Volume { volume: f32 },
    Select { index: usize },
}

enum Operation {
    Snapshot,
    Queue(Vec<Track>, usize),
    Command(PlayerCommand),
}

type Reply = Result<PlayerSnapshot, String>;
enum Request {
    Full(Operation, Sender<Reply>),
    Presentation(Sender<Presentation>),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    pub instance: u64,
    pub generation: u64,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub source_id: Option<String>,
    pub status: Status,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub has_next: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Player {
    requests: Sender<Request>,
}

impl Player {
    pub fn new(
        loader: impl Fn(&Track) -> Result<Media, String> + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let (requests, receiver) = channel();
        let decoder = decode::Worker::new(loader)?;
        thread::Builder::new()
            .name("player-control".into())
            .spawn(move || actor::run(receiver, decoder))
            .map_err(|e| e.to_string())?;
        Ok(Self { requests })
    }

    fn request(&self, operation: Operation) -> Reply {
        let (reply, response) = channel();
        self.requests
            .send(Request::Full(operation, reply))
            .map_err(|_| "Player is unavailable.")?;
        response.recv().map_err(|_| "Player is unavailable.")?
    }

    pub fn snapshot(&self) -> Reply {
        self.request(Operation::Snapshot)
    }

    pub fn presentation(&self) -> Result<Presentation, String> {
        let (reply, response) = channel();
        self.requests
            .send(Request::Presentation(reply))
            .map_err(|_| "Player is unavailable.")?;
        response.recv().map_err(|_| "Player is unavailable.".into())
    }

    pub fn set_queue(&self, tracks: Vec<Track>, start_index: usize) -> Reply {
        self.request(Operation::Queue(tracks, start_index))
    }

    pub fn command(&self, command: PlayerCommand) -> Reply {
        self.request(Operation::Command(command))
    }
}
