use crate::connection::Desktop;
use musicfs_player_core::{PlayerCommand, PlayerSnapshot, Track as QueueTrack};
use musicfs_server_adapter::{Page, Playlist, Track};
use serde::Deserialize;
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Collection {
    Library,
    Favorites,
    History,
}

#[tauri::command]
pub async fn list_tracks(
    state: State<'_, Desktop>,
    collection: Collection,
    query: String,
    cursor: Option<String>,
) -> Result<Page<Track>, String> {
    let server = state.connection()?.server;
    match collection {
        Collection::Library => server.tracks(&query, cursor.as_deref()).await,
        Collection::Favorites => server.favorites(cursor.as_deref()).await,
        Collection::History => server.history(cursor.as_deref()).await,
    }
}

#[tauri::command]
pub async fn list_playlists(
    state: State<'_, Desktop>,
    cursor: Option<String>,
) -> Result<Page<Playlist>, String> {
    state
        .connection()?
        .server
        .playlists(cursor.as_deref())
        .await
}

#[tauri::command]
pub async fn playlist_tracks(
    state: State<'_, Desktop>,
    id: String,
    cursor: Option<String>,
    revision: u64,
) -> Result<Page<Track>, String> {
    state
        .connection()?
        .server
        .playlist_tracks(&id, revision, cursor.as_deref())
        .await
}

#[tauri::command]
pub async fn set_favorite(
    state: State<'_, Desktop>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    state.connection()?.server.set_favorite(&id, enabled).await
}

#[tauri::command]
pub async fn cover(state: State<'_, Desktop>, id: String) -> Result<String, String> {
    state.connection()?.server.cover(&id).await
}

#[tauri::command]
pub async fn player_snapshot(state: State<'_, Desktop>) -> Result<PlayerSnapshot, String> {
    state.connection()?.player.snapshot()
}

#[tauri::command]
pub async fn play_tracks(
    state: State<'_, Desktop>,
    tracks: Vec<QueueTrack>,
    index: usize,
) -> Result<(), String> {
    state
        .connection()?
        .player
        .set_queue(tracks, index)
        .map(|_| ())
}

#[tauri::command]
pub async fn player_command(
    state: State<'_, Desktop>,
    command: PlayerCommand,
) -> Result<(), String> {
    state.connection()?.player.command(command).map(|_| ())
}
