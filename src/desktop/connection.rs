use musicfs_player_core::{Media, Player, PlayerCommand, Track, SUPPORTED_MIMES};
use musicfs_server_adapter::ServerSession;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::runtime::Handle;

pub struct Connection {
    pub server: ServerSession,
    pub player: Player,
    pub force_processing: Arc<AtomicBool>,
    pub saved_login: Option<String>,
    pub new_login: Option<crate::model::credentials::NewLogin>,
    pub login_notice: Option<String>,
}

impl Connection {
    pub async fn from_session(server: ServerSession) -> Result<Self, String> {
        let playback = server.clone();
        let runtime = Handle::current();
        let force_processing = Arc::new(AtomicBool::new(false));
        let preference = force_processing.clone();
        match Player::new(move |track| {
            load(
                &runtime,
                &playback,
                track,
                preference.load(Ordering::Acquire),
            )
        }) {
            Ok(player) => Ok(Self {
                server,
                player,
                force_processing,
                saved_login: None,
                new_login: None,
                login_notice: None,
            }),
            Err(error) => {
                let _ = server.logout().await;
                Err(error)
            }
        }
    }

    pub async fn close(self) -> Result<(), String> {
        let stopped = self.player.command(PlayerCommand::Clear);
        self.server.logout().await?;
        stopped.map(|_| ())
    }
}

// 在播放器的加载线程打开授权的固定音频表示。设备线程只消费 PCM。
fn load(
    runtime: &Handle,
    server: &ServerSession,
    track: &Track,
    force_processing: bool,
) -> Result<Media, String> {
    let source = track
        .source_track_id
        .as_deref()
        .ok_or("This track has no available source.")?;
    let lease = runtime.block_on(server.create_playback(
        if track.source_only { "" } else { &track.id },
        source,
        track.forced_source_track_id.as_deref(),
        SUPPORTED_MIMES,
        force_processing,
    ))?;
    let reader = match server.open_audio_reader(&lease) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = runtime.block_on(server.close_playback(&lease));
            return Err(error);
        }
    };
    let started = (runtime.clone(), server.clone(), lease.clone());
    let closing = (runtime.clone(), server.clone());
    Ok(Media {
        source_track_id: lease.source_id.clone(),
        seek_range_ms: lease.seek_range_ms,
        seekable: reader.byte_seekable(),
        byte_length: reader.byte_length(),
        reader: Box::new(reader),
        on_started: Box::new(move || {
            let (runtime, server, lease) = started;
            runtime.spawn(async move {
                let _ = server.record_started(&lease).await;
            });
        }),
        on_close: Box::new(move || {
            let (runtime, server) = closing;
            runtime.spawn(async move {
                let _ = server.close_playback(&lease).await;
            });
        }),
    })
}
