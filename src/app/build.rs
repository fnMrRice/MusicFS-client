fn main() {
    const COMMANDS: &[&str] = &[
        "current_session",
        "connect",
        "disconnect",
        "list_tracks",
        "list_playlists",
        "playlist_tracks",
        "set_favorite",
        "cover",
        "player_snapshot",
        "play_tracks",
        "player_command",
        "open_compact",
        "restore_main",
        "compact_snapshot",
        "current_lyrics",
        "categories",
        "category_tracks",
        "add_to_playlist",
        "rebuild_playlist",
        "content_types",
        "list_works",
        "work_detail",
        "work_history",
        "recommendation_feed",
        "recommendation_playlist",
    ];
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("Could not build MusicFS Desktop");
}
