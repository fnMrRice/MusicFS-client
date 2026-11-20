use crate::connection::Desktop;
use musicfs_server_adapter::{Page, Work, WorkDetail, WorkHistoryEntry, WorkType};
use tauri::State;

#[tauri::command]
pub async fn content_types(state: State<'_, Desktop>) -> Result<Vec<WorkType>, String> {
    state.connection()?.server.content_types().await
}

#[tauri::command]
pub async fn list_works(
    state: State<'_, Desktop>,
    content_type: String,
    query: String,
    cursor: Option<String>,
) -> Result<Page<Work>, String> {
    state
        .connection()?
        .server
        .works(&content_type, &query, cursor.as_deref())
        .await
}

#[tauri::command]
pub async fn work_detail(state: State<'_, Desktop>, work_id: String) -> Result<WorkDetail, String> {
    state.connection()?.server.work_detail(&work_id).await
}

#[tauri::command]
pub async fn work_history(
    state: State<'_, Desktop>,
    content_type: String,
) -> Result<Vec<WorkHistoryEntry>, String> {
    state.connection()?.server.work_history(&content_type).await
}
