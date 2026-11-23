use musicfs_server_adapter::ServerSession;
use std::io::{Read, Seek, SeekFrom};

#[tokio::test]
async fn rejects_unsafe_or_ambiguous_endpoints_before_login() {
    for endpoint in [
        "http://192.0.2.1",
        "https://bad:secret@example.test",
        "https://example.test/?token=a",
        "https://example.test/api/v1/auth",
        "file:///tmp/music",
    ] {
        assert!(ServerSession::login(endpoint, "example").await.is_err());
    }
}

// 使用专用测试实例，库内至少包含一个可按字节读取的 WAV/MP3/FLAC。
// 该用例会更新该实例测试账号的收藏，并创建一份测试歌单。
#[tokio::test]
#[ignore = "requires an isolated MusicFS server and a seeded music library"]
async fn isolated_server_round_trip() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").expect("Missing test endpoint");
    let credential = std::env::var("MUSICFS_TEST_TOKEN").expect("Missing test credential");
    let session = ServerSession::login(&endpoint, &credential).await.unwrap();
    let feed = session.recommendations(0, false).await.unwrap();
    assert!(feed.history_depth <= 5);
    let tracks = session.tracks("", None).await.unwrap();
    let track = tracks
        .items
        .iter()
        .find(|t| t.source_track_id.is_some())
        .expect("Missing playable test track");
    assert!(session
        .tracks(&track.title, None)
        .await
        .unwrap()
        .items
        .iter()
        .any(|t| t.id == track.id));
    session.set_favorite(&track.id, true).await.unwrap();
    assert!(session
        .favorites(None)
        .await
        .unwrap()
        .items
        .iter()
        .any(|t| t.id == track.id));
    session.set_favorite(&track.id, false).await.unwrap();
    let source_ids = vec![track.source_track_id.clone().unwrap()];
    let operation = uuid::Uuid::new_v4().to_string();
    let created = session
        .create_playlist("Desktop integration", &source_ids, &operation)
        .await
        .unwrap();
    let replayed = session
        .create_playlist("Desktop integration", &source_ids, &operation)
        .await
        .unwrap();
    assert_eq!(created.playlist_id, replayed.playlist_id);
    let updated = session
        .append_playlist(
            &created.playlist_id,
            created.rule_revision,
            &source_ids,
            &uuid::Uuid::new_v4().to_string(),
        )
        .await
        .unwrap();
    let revision = session
        .rebuild_playlist(
            &created.playlist_id,
            updated.rule_revision,
            &uuid::Uuid::new_v4().to_string(),
        )
        .await
        .unwrap();
    let playlist = session
        .playlists(None)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|p| p.id == created.playlist_id)
        .unwrap();
    assert!(!playlist.stale);
    assert_eq!(revision, playlist.snapshot_revision);
    let entries = session
        .playlist_tracks(&playlist.id, revision, None)
        .await
        .unwrap();
    assert!(entries
        .items
        .iter()
        .any(|t| t.source_track_id == track.source_track_id));
    session.lyrics(&source_ids[0]).await.unwrap();

    let lease = session
        .create_playback(
            &track.id,
            track.source_track_id.as_deref().unwrap(),
            None,
            &["audio/wav", "audio/mpeg", "audio/flac"],
            false,
        )
        .await
        .unwrap();
    let source = session.clone();
    let media = lease.clone();
    let mut reader = tokio::task::spawn_blocking(move || source.open_audio_reader(&media))
        .await
        .unwrap()
        .unwrap();
    tokio::task::spawn_blocking(move || {
        assert!(reader.byte_seekable());
        assert!(reader.byte_length().unwrap() > 16);
        let mut prefix = [0u8; 16];
        reader.read_exact(&mut prefix).unwrap();
        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut same = [0u8; 16];
        reader.read_exact(&mut same).unwrap();
        assert_eq!(prefix, same);
    })
    .await
    .unwrap();
    // 此处只验证读取，不模拟已经开始的实际设备输出。
    session.history(None).await.unwrap();
    session.close_playback(&lease).await.unwrap();
    session.logout().await.unwrap();
    assert!(session.tracks("", None).await.is_err());
}
