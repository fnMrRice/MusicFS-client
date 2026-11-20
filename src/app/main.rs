#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod compact;
mod connection;
mod library;

use commands::*;
use compact::*;
use connection::{connect, current_session, disconnect, Desktop};
use library::*;
use std::time::Duration;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(Desktop::default())
        .invoke_handler(tauri::generate_handler![
            current_session, connect, disconnect, list_tracks, list_playlists, playlist_tracks,
            set_favorite, cover, player_snapshot, play_tracks, player_command,
            open_compact, restore_main, compact_snapshot, current_lyrics, categories, category_tracks,
            add_to_playlist, rebuild_playlist
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .build(tauri::generate_context!())
        .expect("Could not initialize MusicFS Desktop")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(async {
                    let desktop = app.state::<Desktop>();
                    let _ = tokio::time::timeout(Duration::from_secs(4), desktop.close()).await;
                });
            }
        });
}
