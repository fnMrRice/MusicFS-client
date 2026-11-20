use crate::connection::Desktop;
use musicfs_server_adapter::{RecommendationFeed, RecommendedPlaylistDetail};
use tauri::State;

#[tauri::command]
pub async fn recommendation_feed(
    state: State<'_, Desktop>,
    history: u8,
    refresh: bool,
) -> Result<RecommendationFeed, String> {
    state
        .connection()?
        .server
        .recommendations(history, refresh)
        .await
}

#[tauri::command]
pub async fn recommendation_playlist(
    state: State<'_, Desktop>,
    instance: String,
    external: String,
    cursor: Option<String>,
) -> Result<RecommendedPlaylistDetail, String> {
    state
        .connection()?
        .server
        .recommendation_playlist(&instance, &external, cursor.as_deref())
        .await
}
