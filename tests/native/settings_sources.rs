use super::*;
use crate::ui::settings::sources::api::list;
use std::time::Duration;

#[test]
fn additions_preserve_existing_root_bindings_and_import_defaults() {
    let config: Config =
        serde_json::from_value(json!({"revision":"r","rootImportDefaultsSupported":true,
        "roots":[{"id":"old","name":"Original","importDefaults":{"recursive":false,"maxDepth":0}}],
        "importOptions":{"importDefaults":{"excludePatterns":["temp/**"],"importArtwork":true}}}))
        .unwrap();
    let mut editor = Editor::new(config, Action::Add);
    editor
        .selected
        .push(("new".into(), "New".into(), "/allowed/new".into()));
    editor.options.max_depth = 3;
    let body = editor.body();
    assert_eq!(body["musicRoots"][0], json!({"id":"old","name":"Original"}));
    assert_eq!(body["musicRoots"][1]["importDefaults"]["maxDepth"], 3);
    assert_eq!(
        body["importDefaults"]["excludePatterns"],
        json!(["temp/**"])
    );
}

// 此用例只对临时实例写入配置，夹具提供一份带子目录的本地音乐源。
#[test]
#[ignore]
fn isolated_sources_directory_scan_and_watch_round_trip() {
    assert_eq!(
        std::env::var("MUSICFS_TEST_SOURCES").as_deref(),
        Ok("isolated")
    );
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
        assert!(endpoint.starts_with("http://127.0.0.1:"));
        let s = ServerSession::login(&endpoint, &std::env::var("MUSICFS_TEST_TOKEN").unwrap())
            .await
            .unwrap();
        let activity = s.listen_activity(|| {});
        let providers: Vec<Value> = list(&s, "/provider-instances").await.unwrap();
        let id = text(
            providers
                .iter()
                .find(|p| {
                    p.pointer("/plugin/id").and_then(Value::as_str)
                        == Some("musicfs.plugin.source.localfiles")
                })
                .unwrap(),
            "id",
        );
        let base = format!("/admin/plugin-instances/{}", path_id(id));
        let config_path = format!("{base}/configuration");
        let first: Config = get(&s, &config_path).await.unwrap();
        let roots: Value = get(&s, &format!("{base}/directories")).await.unwrap();
        let root = roots["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["configuredRootId"] == first.roots[0].id)
            .unwrap();
        let mut params = url::form_urlencoded::Serializer::new(String::new());
        params.append_pair("path", text(root, "path"));
        let children: Value = get(&s, &format!("{base}/directories?{}", params.finish()))
            .await
            .unwrap();
        let child = children["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "native-source-test")
            .unwrap();
        let mut add = Editor::new(first.clone(), Action::Add);
        let root_id = format!("root-{}", Uuid::new_v4());
        add.options.max_depth = 2;
        add.selected.push((
            root_id.clone(),
            "Native import".into(),
            text(child, "path").into(),
        ));
        let op = apply(
            &s,
            activity.subscribe(),
            id,
            &first.revision,
            &Uuid::new_v4().to_string(),
            add.body(),
        )
        .await
        .unwrap();
        let job = scans::auto_scan(&s, id, &op).await.unwrap();
        let read: Config = get(&s, &config_path).await.unwrap();
        assert_eq!(
            read.roots
                .iter()
                .find(|r| r.id == root_id)
                .unwrap()
                .import_defaults
                .as_ref()
                .unwrap()
                .max_depth,
            2
        );
        let mut scans = activity.subscribe();
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let revision = scans.borrow_and_update().scans;
                let job: Value = get(&s, &format!("/scan-jobs/{}", path_id(&job)))
                    .await
                    .unwrap();
                if job["status"] == "succeeded" {
                    break;
                }
                assert!(!matches!(
                    text(&job, "status"),
                    "failed" | "cancelled" | "paused"
                ));
                scans.wait_for(|r| r.scans != revision).await.unwrap();
            }
        })
        .await
        .unwrap();
        let stats: Value = get(
            &s,
            &format!("/provider-instances/{}/root-statistics", path_id(id)),
        )
        .await
        .unwrap();
        assert!(stats["roots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["rootId"] == root_id));
        let mut edit = Editor::new(read.clone(), Action::Edit(root_id.clone()));
        edit.name = "Renamed fixture".into();
        edit.options.import_lyrics = false;
        apply(
            &s,
            activity.subscribe(),
            id,
            &read.revision,
            &Uuid::new_v4().to_string(),
            edit.body(),
        )
        .await
        .unwrap();
        let changed: Config = get(&s, &config_path).await.unwrap();
        let row = changed.roots.iter().find(|r| r.id == root_id).unwrap();
        assert_eq!(row.name, "Renamed fixture");
        assert!(!row.import_defaults.as_ref().unwrap().import_lyrics);
        let mut watch = serde_json::to_value(changed.watch.as_ref().unwrap()).unwrap();
        watch["debounceMs"] = json!(1678);
        apply(
            &s,
            activity.subscribe(),
            id,
            &changed.revision,
            &Uuid::new_v4().to_string(),
            json!({"watch":watch}),
        )
        .await
        .unwrap();
        let after: Config = get(&s, &config_path).await.unwrap();
        assert_eq!(
            serde_json::to_value(after.watch.as_ref().unwrap()).unwrap()["debounceMs"],
            1678
        );
        let remove = Editor::new(after.clone(), Action::Remove(root_id.clone()));
        apply(
            &s,
            activity.subscribe(),
            id,
            &after.revision,
            &Uuid::new_v4().to_string(),
            remove.body(),
        )
        .await
        .unwrap();
        let restored: Config = get(&s, &config_path).await.unwrap();
        assert!(!restored.roots.iter().any(|r| r.id == root_id));
        s.logout().await.unwrap();
    });
}
