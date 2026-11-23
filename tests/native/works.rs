use super::*;
use reqwest::header;
use serde_json::{json, Value};
use std::io::Read;

#[test]
fn package_nodes_preserve_unavailable_and_untagged_audio() {
    let sample = json!({
        "workId":"work", "packageId":"pack", "name":"Disc 1", "kind":"directory",
        "nodes":[
            {"nodeId":"root","parentId":"","name":"Disc 1","path":".","kind":"directory","order":0,"sourceTrackId":"","track":null},
            {"nodeId":"a","parentId":"root","name":"recording.wav","path":"recording.wav","kind":"audio","order":0,"sourceTrackId":"item",
             "track":{"sourceTrackId":"item","artists":null,"title":null,"durationMs":null,"availability":"available"}},
            {"nodeId":"b","parentId":"root","name":"missing.wav","path":"missing.wav","kind":"audio","order":1,"sourceTrackId":"missing","track":null},
            {"nodeId":"c","parentId":"root","name":"notes.txt","path":"notes.txt","kind":"attachment","order":2,"sourceTrackId":"","track":null}
        ]
    });
    let output = serde_json::from_value::<Package>(sample).unwrap().audio();
    assert_eq!(output.audio.len(), 2);
    assert_eq!(output.audio[0].source_track_id.as_deref(), Some("item"));
    assert_eq!(output.audio[0].title, "recording.wav");
    assert!(output.audio[0].artists.is_empty());
    assert!(output.audio[1].source_track_id.is_none());
}

async fn post_fixture(session: &ServerSession, path: &str, body: Value, idempotent: bool) -> Value {
    let mut request = session
        .client
        .post(session.url(path))
        .json(&body)
        .header(header::ORIGIN, &session.info.endpoint)
        .header("X-CSRF-Token", &session.csrf);
    if idempotent {
        request = request.header("Idempotency-Key", uuid::Uuid::new_v4().to_string());
    }
    decode(request.send().await.unwrap()).await.unwrap()
}

// 仅对专用实例执行；指定目录需包含一份 WAV。测试创建作品、导入副本并模拟播放开始回执。
#[tokio::test]
#[ignore = "requires an isolated MusicFS server with an enabled ASMR plugin and WAV fixture root"]
async fn isolated_work_package_round_trip() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").expect("Missing test endpoint");
    let token = std::env::var("MUSICFS_TEST_TOKEN").expect("Missing test token");
    let provider = std::env::var("MUSICFS_TEST_WORK_PROVIDER").expect("Missing test provider");
    let root = std::env::var("MUSICFS_TEST_WORK_ROOT").expect("Missing fixture root ID");
    let path = std::env::var("MUSICFS_TEST_WORK_PATH").expect("Missing fixture subdirectory");
    let session = ServerSession::login(&endpoint, &token).await.unwrap();
    assert!(session
        .content_types()
        .await
        .unwrap()
        .iter()
        .any(|t| t.id == "asmr" && t.separate_history));
    let title = format!("Desktop fixture {}", uuid::Uuid::new_v4());
    let created = post_fixture(&session, "works", json!({
        "contentType":"asmr","title":title,"description":"","externalIds":[],"credits":[],"tags":[]
    }), true).await;
    let work = created["workId"].as_str().unwrap();
    let preview = post_fixture(
        &session,
        &format!("works/{work}/package-previews"),
        json!({
            "providerInstanceId":provider,"rootId":root,"path":path,"kind":"directory"
        }),
        false,
    )
    .await;
    post_fixture(
        &session,
        &format!("works/{work}/packages"),
        json!({"previewId":preview["previewId"]}),
        true,
    )
    .await;
    let listed = session.works("asmr", &title, None).await.unwrap();
    assert!(listed.items.iter().any(|w| w.work_id == work));
    let detail = session.work_detail(work).await.unwrap();
    let source = detail
        .packages
        .iter()
        .flat_map(|p| &p.audio)
        .find_map(|n| n.source_track_id.as_ref())
        .expect("No available package audio");
    let lease = session
        .create_playback("", source, None, &["audio/wav"], false)
        .await
        .unwrap();
    assert_eq!(lease.source_id, *source);
    let server = session.clone();
    let media = lease.clone();
    tokio::task::spawn_blocking(move || {
        let mut reader = server.open_audio_reader(&media).unwrap();
        let mut header = [0; 4];
        reader.read_exact(&mut header).unwrap();
        assert_eq!(&header, b"RIFF");
    })
    .await
    .unwrap();
    session.record_started(&lease).await.unwrap();
    assert!(session
        .work_history("asmr")
        .await
        .unwrap()
        .iter()
        .any(|e| e.work_id == work && e.source_track_id == *source));
    session.close_playback(&lease).await.unwrap();
    session.logout().await.unwrap();
}
