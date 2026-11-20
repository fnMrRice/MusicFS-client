use crate::connection::Desktop;
use musicfs_server_adapter::{Category, CategoryKind, Page, Track};
use serde::Deserialize;
use tauri::State;

#[tauri::command]
pub async fn categories(
    state: State<'_, Desktop>, kind: CategoryKind, cursor: Option<String>,
) -> Result<Page<Category>, String> {
    state.connection()?.server.categories(kind, cursor.as_deref()).await
}

#[tauri::command]
pub async fn category_tracks(
    state: State<'_, Desktop>, kind: CategoryKind, id: String, cursor: Option<String>,
) -> Result<Page<Track>, String> {
    state.connection()?.server.category_tracks(kind, &id, cursor.as_deref()).await
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum PlaylistTarget {
    Existing { id: String, #[serde(rename = "ruleRevision")] rule_revision: u64 },
    New { name: String },
}

#[tauri::command]
pub async fn add_to_playlist(
    state: State<'_, Desktop>, source_id: String, target: PlaylistTarget, key: String,
) -> Result<(), String> {
    let server = state.connection()?.server;
    let selection = [source_id];
    match target {
        PlaylistTarget::Existing { id, rule_revision } => server.append_playlist(&id, rule_revision, &selection, &key).await?,
        PlaylistTarget::New { name } => server.create_playlist(&name, &selection, &key).await?,
    };
    Ok(())
}

#[tauri::command]
pub async fn rebuild_playlist(state: State<'_, Desktop>, id: String, revision: u64, key: String) -> Result<u64, String> {
    state.connection()?.server.rebuild_playlist(&id, revision, &key).await
}
