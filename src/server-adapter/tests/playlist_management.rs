use musicfs_server_adapter::ServerSession;
use serde_json::json;
use uuid::Uuid;

fn key() -> String {
    Uuid::new_v4().to_string()
}

// 只在含扫描样本的隔离服务上创建和修改歌单，覆盖空歌单及已有来源绑定的保存流程。
#[tokio::test]
#[ignore = "requires a disposable local MusicFS server with a scanned track"]
async fn manages_empty_and_aggregated_playlists() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let session = ServerSession::login(&endpoint, &std::env::var("MUSICFS_TEST_TOKEN").unwrap())
        .await
        .unwrap();
    let name = format!("Desktop {}", key());
    let operation = key();
    for _ in 0..2 {
        session
            .save_playlist_rules(None, &name, &[], &operation)
            .await
            .unwrap();
    }
    let playlists = session.playlists(None).await.unwrap().items;
    let mut matches = playlists.iter().filter(|p| p.name == name);
    let created = matches.next().unwrap();
    assert!(matches.next().is_none());
    let (playlist, bindings) = session.playlist_rules(&created.id).await.unwrap();
    assert!(bindings.is_empty());
    let track = session
        .tracks("", None)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|t| t.playable_source_id().is_some())
        .unwrap();
    let source = track.playable_source_id().unwrap().to_owned();
    let other = session
        .create_playlist("Source playlist", std::slice::from_ref(&source), &key())
        .await
        .unwrap();
    let rules = vec![
        json!({"kind":"playlist","targetId":other.playlist_id,"entries":[]}),
        json!({"kind":"local","entries":[{"source":{"trackId":track.id,"forcedSourceTrackId":source}}]}),
    ];
    let operation = key();
    for _ in 0..2 {
        session
            .save_playlist_rules(
                Some((&playlist.id, playlist.rule_revision)),
                "Renamed",
                &rules,
                &operation,
            )
            .await
            .unwrap();
    }
    let (renamed, mut rules) = session.playlist_rules(&created.id).await.unwrap();
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.rule_revision, created.rule_revision + 1);
    assert_eq!(rules[0]["targetId"], other.playlist_id);
    let entry = rules[1]["entries"][0].clone();
    let resolved = session
        .playlist_rule_entries(vec![entry.clone()])
        .await
        .unwrap();
    assert_eq!(resolved[0].title, track.title);
    assert_eq!(
        resolved[0].forced_source_track_id.as_deref(),
        Some(source.as_str())
    );
    rules.swap(0, 1);
    rules[0]["entries"][0]["source"]["forcedSourceTrackId"] = serde_json::Value::Null;
    let stale = session
        .save_playlist_rules(
            Some((&playlist.id, playlist.rule_revision)),
            "Old version",
            &[],
            &key(),
        )
        .await
        .unwrap_err();
    assert!(stale.contains("409"), "{stale}");
    session
        .save_playlist_rules(
            Some((&renamed.id, renamed.rule_revision)),
            &renamed.name,
            &rules,
            &key(),
        )
        .await
        .unwrap();
    let (reordered, mut saved) = session.playlist_rules(&created.id).await.unwrap();
    assert_eq!(saved[0]["entries"][0]["entryId"], entry["entryId"]);
    assert_eq!(saved[0]["entries"][0]["source"]["trackId"], track.id);
    assert!(saved[0]["entries"][0]["source"]["forcedSourceTrackId"].is_null());
    assert_eq!(saved[1]["targetId"], other.playlist_id);
    saved.remove(1);
    session
        .save_playlist_rules(
            Some((&created.id, reordered.rule_revision)),
            &renamed.name,
            &saved,
            &key(),
        )
        .await
        .unwrap();
    let (edited, saved) = session.playlist_rules(&created.id).await.unwrap();
    assert_eq!(saved.len(), 1);
    let revision = session
        .rebuild_playlist(&created.id, edited.rule_revision, &key())
        .await
        .unwrap();
    let published = session
        .playlist_tracks(&created.id, revision, None)
        .await
        .unwrap();
    assert_eq!(published.items.len(), 1);
    assert_eq!(published.items[0].id, track.id);
}
