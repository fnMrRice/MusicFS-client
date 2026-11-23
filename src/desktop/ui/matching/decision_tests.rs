use super::*;
use std::time::{Duration, Instant};

#[test]
fn scopes_bulk_rules_and_idempotency_keys() {
    let (path, score) = Decision::Bulk { score: Some(80) }.payload(5);
    assert_eq!(path, "bulk-merge");
    assert_eq!(
        score,
        json!({"expectedRevision":5,"mode":"score","minimumScore":80})
    );
    assert_eq!(
        Decision::Bulk { score: None }.payload(5).1,
        json!({"expectedRevision":5,"mode":"title_artist"})
    );
    assert!(serde_json::from_value::<UuidKey>(json!("../settings")).is_err());
}

// 使用隔离曲库中的相同 WAV 副本，覆盖真实候选、批量处理、排除、手动关联与幂等重放。
#[test]
#[ignore]
fn isolated_duplicate_decisions_and_recovery() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    assert!(std::env::var_os("MUSICFS_DESKTOP_CONFIG_DIR").is_some());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let server = ServerSession::login(&endpoint, &std::env::var("MUSICFS_TEST_TOKEN").unwrap())
            .await
            .unwrap();
        let rows = server
            .settings_json("GET", "/source-tracks", None, &[])
            .await
            .unwrap();
        let sources = rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| {
                t["displayName"]
                    .as_str()
                    .is_some_and(|n| matches!(n, "markers.wav" | "markers-copy.wav"))
            })
            .map(|t| t["sourceTrackId"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), 2, "Need two dedicated duplicate fixtures");
        for id in &sources {
            let job = server
                .settings_json(
                    "POST",
                    &format!("/source-tracks/{id}/hash-jobs"),
                    Some(&json!({})),
                    &[("Idempotency-Key", &Uuid::new_v4().to_string())],
                )
                .await
                .unwrap();
            let path = format!("/hash-jobs/{}", job["jobId"].as_str().unwrap());
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                let job = server.settings_json("GET", &path, None, &[]).await.unwrap();
                if job["status"] == "succeeded" {
                    break;
                }
                assert!(
                    job["status"] != "failed" && Instant::now() < deadline,
                    "Fixture hashes unavailable"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        server
            .settings_json(
                "POST",
                &format!("/source-tracks/{}/match-candidates/refresh", sources[0]),
                Some(&json!({"sourceTrackIds":[sources[1]]})),
                &[],
            )
            .await
            .unwrap();
        let page = server
            .settings_json("GET", "/matching/candidates", None, &[])
            .await
            .unwrap();
        assert!(page["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["score"] == 100));
        let make = |revision, request| Pending {
            endpoint: endpoint.clone(),
            instance: server.info.instance_id.clone(),
            user: server.info.user_name.clone(),
            revision,
            key: UuidKey(Uuid::new_v4()),
            request,
        };
        let revision = server.matching_revision().await.unwrap();
        let bulk = make(revision, Decision::Bulk { score: Some(100) });
        bulk.persist().unwrap();
        let first = submit(&server, &bulk).await.unwrap();
        assert!(first.merged_groups.is_some_and(|v| v > 0));
        let restored = Pending::restore(&server.info).unwrap();
        assert_eq!(restored.key.0, bulk.key.0);
        let replay = submit(&server, &restored).await.unwrap();
        assert_eq!(replay.operation_id, first.operation_id);
        let mut recovery = Decisions {
            pending: Some(restored),
            ..Default::default()
        };
        recovery.clear();
        assert!(Pending::restore(&server.info).is_none());
        let mut rev = replay.revision;
        for action in [
            Decision::Unlink(sources[1].clone()),
            Decision::Exclude {
                left: sources[0].clone(),
                right: sources[1].clone(),
                excluded: true,
            },
            Decision::Exclude {
                left: sources[0].clone(),
                right: sources[1].clone(),
                excluded: false,
            },
            Decision::Link {
                sources: sources.clone(),
                target: None,
            },
            Decision::Bulk { score: None },
        ] {
            rev = submit(&server, &make(rev, action)).await.unwrap().revision;
        }
        assert_eq!(server.matching_revision().await.unwrap(), rev);
        server.logout().await.unwrap();
    });
}
