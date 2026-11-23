use musicfs_server_adapter::ServerSession;
use uuid::Uuid;

fn key() -> String {
    Uuid::new_v4().to_string()
}

// 测试只在专用回环实例中运行，会写入收藏并创建一个歌单。
#[tokio::test]
#[ignore = "requires an isolated MusicFS server with a scanned audio file"]
async fn song_references_round_trip_with_and_without_a_pin() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let session = ServerSession::login(&endpoint, &std::env::var("MUSICFS_TEST_TOKEN").unwrap())
        .await
        .unwrap();
    let track = session
        .sourced_tracks(None, None)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|t| t.playable_source_id().is_some())
        .unwrap();
    let src = track.source().unwrap();
    assert!(session
        .sourced_tracks(Some(&src.provider_instance_id), None)
        .await
        .unwrap()
        .items
        .iter()
        .any(|t| t.id == track.id));
    let pin = src.source_track_id.as_str();
    session
        .favorite_version(&track.id, Some(pin))
        .await
        .unwrap();
    let favorite = session
        .favorites(None)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|t| t.id == track.id)
        .unwrap();
    assert_eq!(favorite.forced_source_track_id.as_deref(), Some(pin));
    session.favorite_version(&track.id, None).await.unwrap();
    assert!(session
        .favorites(None)
        .await
        .unwrap()
        .items
        .iter()
        .find(|t| t.id == track.id)
        .unwrap()
        .forced_source_track_id
        .is_none());

    let name = key();
    let operation = key();
    session
        .add_track_reference(None, &name, &track.id, None, &operation)
        .await
        .unwrap();
    session
        .add_track_reference(None, &name, &track.id, None, &operation)
        .await
        .unwrap();
    let playlists = session.playlists(None).await.unwrap();
    assert_eq!(playlists.items.iter().filter(|p| p.name == name).count(), 1);
    let list = playlists.items.iter().find(|p| p.name == name).unwrap();
    session
        .add_track_reference(
            Some((&list.id, list.rule_revision)),
            "",
            &track.id,
            Some(pin),
            &key(),
        )
        .await
        .unwrap();
    let detail = session
        .settings_json("GET", &format!("/playlists/{}", list.id), None, &[])
        .await
        .unwrap();
    let bindings = detail["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0]["entries"][0]["source"]["trackId"], track.id);
    assert!(bindings[0]["entries"][0]["source"]["forcedSourceTrackId"].is_null());
    assert_eq!(
        bindings[1]["entries"][0]["source"]["forcedSourceTrackId"],
        pin
    );
    let revision = session
        .rebuild_playlist(&list.id, detail["ruleRevision"].as_u64().unwrap(), &key())
        .await
        .unwrap();
    let saved = session
        .playlist_tracks(&list.id, revision, None)
        .await
        .unwrap()
        .items;
    assert_eq!(saved.len(), 2);
    assert!(saved
        .iter()
        .any(|t| t.id == track.id && t.forced_source_track_id.is_none()));
    assert!(saved
        .iter()
        .any(|t| t.id == track.id && t.forced_source_track_id.as_deref() == Some(pin)));
    session.logout().await.unwrap();
}
