use crate::connection::Desktop;
use musicfs_player_core::Presentation;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

fn focus(window: &WebviewWindow) -> Result<(), String> {
    window.show().and_then(|_| window.unminimize()).and_then(|_| window.set_focus())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_compact(app: AppHandle, kind: String) -> Result<(), String> {
    let (title, width, height, min_width, min_height) = match kind.as_str() {
        "mini" => ("MusicFS · 迷你播放器", 430.0, 164.0, 330.0, 150.0),
        "lyrics" => ("MusicFS · 桌面歌词", 660.0, 224.0, 360.0, 160.0),
        _ => return Err("Unknown player window.".into()),
    };
    if let Some(window) = app.get_webview_window(&kind) { return focus(&window); }
    let url = WebviewUrl::App(format!("index.html#/{kind}").into());
    match WebviewWindowBuilder::new(&app, &kind, url)
        .title(title).inner_size(width, height).min_inner_size(min_width, min_height)
        .center().always_on_top(true).build() {
        Ok(_) => Ok(()),
        Err(e) => match app.get_webview_window(&kind) {
            Some(window) => focus(&window),
            None => Err(e.to_string()),
        },
    }
}

#[tauri::command]
pub async fn restore_main(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("Main window is unavailable.")?;
    focus(&main)?;
    if matches!(window.label(), "mini" | "lyrics") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn compact_snapshot(state: State<'_, Desktop>) -> Result<Option<Presentation>, String> {
    match state.connection() {
        Ok(connection) => connection.player.presentation().map(Some),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn current_lyrics(state: State<'_, Desktop>) -> Result<Option<String>, String> {
    let connection = state.connection()?;
    let before = connection.player.presentation()?;
    let Some(source) = &before.source_id else { return Ok(None); };
    let text = connection.server.lyrics(source).await?;
    let after = state.connection()?.player.presentation()?;
    if after.instance != before.instance || after.source_id != before.source_id { return Ok(None); }
    Ok(text)
}
