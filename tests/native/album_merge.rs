use super::*;

fn item(id: &str, title: &str, names: &[&str], aliases: Value, extra: Value) -> Item {
    let mut v = json!({"trackId":id,"title":title,"revision":1,"artists":names.iter().map(|name|json!({"name":name})).collect::<Vec<_>>(),
        "albums":[{"albumId":"album"}],"localizedTitles":aliases});
    for (k, value) in extra.as_object().unwrap() {
        v[k] = value.clone();
    }
    serde_json::from_value(v).unwrap()
}

#[test]
fn album_membership_uses_full_ids_even_when_credits_are_folded() {
    let track = |extra| item("track", "Song", &[], json!([]), extra);
    let folded = track(json!({"albumIds": ["album", "other-source"]}));
    assert!(folded.in_album("other-source"));
    assert!(!folded.in_album("unrelated"));
    assert!(!track(json!({"albumIds": []})).in_album("album"));
    assert!(track(json!({})).in_album("album"));
}

#[test]
fn merged_metadata_prefers_primary_and_preserves_titles_and_credits() {
    let items = vec![
        item(
            "donor",
            "Other title",
            &["A", "B"],
            json!([{"language":"en","title":"Alias"}]),
            json!({"rating":5,"year":1998}),
        ),
        item(
            "main",
            "Primary",
            &["B", "C"],
            json!([{"language":"EN","title":"Alias"}]),
            json!({"rating":3}),
        ),
    ];
    let auto = Draft::auto(&items, 1, None).wire().unwrap();
    assert_eq!(auto["artistNames"], json!(["B", "C", "A"]));
    assert_eq!(auto["rating"], 3);
    assert_eq!(auto["year"], 1998);
    assert_eq!(
        auto["localizedTitles"],
        json!([{"language":"EN","title":"Alias"},{"language":"und","title":"Other title"}])
    );
    let mut chosen = Draft::auto(&items, 1, Some("New name"));
    chosen.title = "New name".into();
    chosen.assign(Field::Rating, &items[0].draft());
    let custom = chosen.wire().unwrap();
    assert_eq!(custom["rating"], 5);
    assert!(custom["localizedTitles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["title"] == "Primary"));
}

// 仅针对隔离实例创建和合并测试资料，同时确认过期修订及相同幂等键重试。
#[test]
#[ignore]
fn isolated_album_track_merge_round_trip() {
    assert_eq!(
        std::env::var("MUSICFS_TEST_ALBUM_MERGE").as_deref(),
        Ok("isolated")
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
        assert!(endpoint.starts_with("http://127.0.0.1:"));
        let s = ServerSession::login(&endpoint, &std::env::var("MUSICFS_TEST_TOKEN").unwrap())
            .await
            .unwrap();
        let album = s
            .settings_json(
                "POST",
                "/library/albums",
                Some(&json!({"name":"Native merge fixture"})),
                &[("Idempotency-Key", &Uuid::new_v4().to_string())],
            )
            .await
            .unwrap();
        let id = album["albumId"].as_str().unwrap();
        let mut ids = vec![];
        for (title, artist) in [("First", "Alice"), ("Second", "Bob")] {
            let t = s
                .settings_json(
                    "POST",
                    "/library/tracks",
                    Some(&json!({"title":title,"artistNames":[artist],"albumIds":[id]})),
                    &[("Idempotency-Key", &Uuid::new_v4().to_string())],
                )
                .await
                .unwrap();
            ids.push(t["trackId"].as_str().unwrap().to_owned());
        }
        let selection = load_selection(&s, id, &ids).await.unwrap();
        let mut draft = Draft::auto(&selection.items, 0, Some("Combined"));
        draft.title = "Combined".into();
        let request = payload(&selection, 0, draft.wire().unwrap());
        let mut stale = request.clone();
        stale["tracks"][0]["expectedRevision"] = json!(selection.items[0].revision + 1);
        assert!(save(&s, id, &stale, &Uuid::new_v4().to_string())
            .await
            .is_err());
        let key = Uuid::new_v4().to_string();
        save(&s, id, &request, &key).await.unwrap();
        save(&s, id, &request, &key).await.unwrap();
        let merged = s
            .settings_json("GET", &format!("/library/tracks/{}", ids[0]), None, &[])
            .await
            .unwrap();
        assert_eq!(merged["title"], "Combined");
        assert_eq!(merged["artists"].as_array().unwrap().len(), 2);
        assert!(merged["localizedTitles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["title"] == "Second"));
        s.logout().await.unwrap();
    });
}
