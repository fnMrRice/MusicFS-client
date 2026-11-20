use musicfs_player_core::{Media, Player, PlayerCommand, Track, SUPPORTED_MIMES};
use musicfs_server_adapter::{ServerSession, SessionInfo};
use std::sync::RwLock;
use tauri::State;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Connection {
    pub server: ServerSession,
    pub player: Player,
}

#[derive(Default)]
pub struct Desktop {
    connection: RwLock<Option<Connection>>,
    transition: Mutex<()>,
}

impl Desktop {
    pub fn connection(&self) -> Result<Connection, String> {
        self.connection
            .read()
            .map_err(|_| "Connection unavailable.")?
            .clone()
            .ok_or_else(|| "Sign in to MusicFS first.".into())
    }

    pub async fn close(&self) -> Result<(), String> {
        let _guard = self.transition.lock().await;
        let previous = self
            .connection
            .write()
            .map_err(|_| "Connection unavailable.")?
            .take();
        if let Some(previous) = previous {
            let stopped = previous.player.command(PlayerCommand::Clear);
            previous.server.logout().await?;
            stopped?;
        }
        Ok(())
    }
}

fn load(server: &ServerSession, track: &Track) -> Result<Media, String> {
    let source = track
        .source_track_id
        .as_deref()
        .ok_or("This track has no available source.")?;
    let lease = tauri::async_runtime::block_on(server.create_playback(
        if track.source_only { "" } else { &track.id },
        source,
        track.forced_source_track_id.as_deref(),
        SUPPORTED_MIMES,
    ))?;
    let reader = match server.open_audio_reader(&lease) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = tauri::async_runtime::block_on(server.close_playback(&lease));
            return Err(error);
        }
    };
    let started_server = server.clone();
    let started_lease = lease.clone();
    let closing_server = server.clone();
    Ok(Media {
        source_track_id: lease.source_id.clone(),
        seek_range_ms: lease.seek_range_ms,
        seekable: reader.byte_seekable(),
        byte_length: reader.byte_length(),
        reader: Box::new(reader),
        on_started: Box::new(move || {
            tauri::async_runtime::spawn(async move {
                let _ = started_server.record_started(&started_lease).await;
            });
        }),
        on_close: Box::new(move || {
            tauri::async_runtime::spawn(async move {
                let _ = closing_server.close_playback(&lease).await;
            });
        }),
    })
}

#[tauri::command]
pub fn current_session(state: State<'_, Desktop>) -> Result<Option<SessionInfo>, String> {
    let guard = state
        .connection
        .read()
        .map_err(|_| "Connection unavailable.")?;
    Ok(guard.as_ref().map(|c| c.server.info.clone()))
}

#[tauri::command]
pub async fn connect(
    state: State<'_, Desktop>,
    endpoint: String,
    token: String,
) -> Result<SessionInfo, String> {
    let _guard = state.transition.lock().await;
    if state
        .connection
        .read()
        .map_err(|_| "Connection unavailable.")?
        .is_some()
    {
        return Err("Disconnect before changing servers.".into());
    }
    let server = ServerSession::login(&endpoint, &token).await?;
    let playback = server.clone();
    let player = match Player::new(move |track| load(&playback, track)) {
        Ok(player) => player,
        Err(error) => {
            let _ = server.logout().await;
            return Err(error);
        }
    };
    let info = server.info.clone();
    *state
        .connection
        .write()
        .map_err(|_| "Connection unavailable.")? = Some(Connection { server, player });
    Ok(info)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, Desktop>) -> Result<(), String> {
    state.close().await
}
