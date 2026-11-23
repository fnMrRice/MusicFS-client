use super::*;
use std::collections::HashSet;

// 夹具先扫描含 104 份带不同来源身份的短 WAV 的目录；测试使用隔离服务。
#[tokio::test]
#[ignore]
async fn isolated_folder_paging_and_fixed_source_playback() {
    assert_eq!(
        std::env::var("MUSICFS_TEST_FOLDERS").as_deref(),
        Ok("isolated")
    );
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let server = ServerSession::login(&endpoint, &std::env::var("MUSICFS_TEST_TOKEN").unwrap())
        .await
        .unwrap();
    let roots = server.folders(None, None, "", None).await.unwrap();
    let root = roots.items.iter().find(|f| f.root_id == "local").unwrap();
    let children = server
        .folders(
            Some(&root.provider_instance_id),
            Some(&root.category_id),
            "",
            None,
        )
        .await
        .unwrap();
    let folder = children
        .items
        .iter()
        .find(|f| f.name == "folder-pages-test")
        .unwrap();
    let first = server
        .folder_recordings(
            Some(&root.provider_instance_id),
            &folder.category_id,
            "",
            None,
        )
        .await
        .unwrap();
    assert_eq!(first.total, 104);
    assert_eq!(first.items.len(), 100);
    let mut ids: HashSet<String> = first.items.into_iter().map(|t| t.source_track_id).collect();
    let last = server
        .folder_recordings(
            Some(&root.provider_instance_id),
            &folder.category_id,
            "",
            first.next,
        )
        .await
        .unwrap();
    assert_eq!(last.items.len(), 4);
    assert!(last.next.is_none());
    for track in &last.items {
        assert!(ids.insert(track.source_track_id.clone()));
    }
    let chosen = &last.items[0];
    let lease = server
        .create_playback(
            "",
            &chosen.source_track_id,
            None,
            &["audio/wav", "audio/x-wav"],
            false,
        )
        .await
        .unwrap();
    assert_eq!(lease.source_id, chosen.source_track_id);
    server.close_playback(&lease).await.unwrap();
    let exact = server
        .folder_recordings(None, &folder.category_id, "sample-001", None)
        .await
        .unwrap();
    assert_eq!(exact.total, 1);
    server.logout().await.unwrap();
}
