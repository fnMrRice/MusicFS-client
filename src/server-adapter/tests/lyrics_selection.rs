use musicfs_server_adapter::ServerSession;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

#[tokio::test]
async fn search_and_apply_use_the_selected_candidate_and_original_asset_revision() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let target = json!({"sourceTrackId":"track-a","assetId":"asset-a","assetVersion":"v1"});
        let media = json!({"target":target,"revision":4,"bindings":[]});
        let saved = json!({"target":target,"revision":5,"bindings":[]});
        let candidates = json!({"pluginId":"source","items":[
            {"candidateToken":"first","title":"Song","lyrics":"Original"},
            {"candidateToken":"second","title":"Song","lyrics":"Translation"}
        ]});
        let query = json!({"sourceTrackId":"track-a","query":{"title":"Song","artist":"A, B","duration":3.25}});
        let choice = json!({"assetId":"asset-a","assetVersion":"v1","expectedRevision":4,"pluginId":"source","candidateToken":"second"});
        let apply = "PUT /api/v1/source-tracks/track-a/scraped-fields/lyrics";
        let cases = [
            (
                "GET /api/v1/health/ready",
                None,
                json!({"ready":true,"product":"MusicFS","apiVersion":"1","instanceId":"fixture","serverVersion":"1"}),
            ),
            (
                "POST /api/v1/auth/session",
                None,
                json!({"user":{"id":"fixture","role":"user"},"csrfToken":"test-csrf"}),
            ),
            (
                "GET /api/v1/source-tracks/track-a/media-selection",
                None,
                media,
            ),
            (
                "POST /api/v1/scrapers/lyrics-auto-search",
                Some(&query),
                candidates,
            ),
            (apply, Some(&choice), saved.clone()),
            (apply, Some(&choice), saved),
        ];
        for (request, expected_body, reply) in cases {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(&stream);
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            assert_eq!(first.trim_end(), format!("{request} HTTP/1.1"));
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line.to_ascii_lowercase());
            }
            if let Some(expected) = expected_body {
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                assert_eq!(&serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
                assert!(headers.contains("x-csrf-token: test-csrf"));
            }
            if request == apply {
                assert!(headers.contains("idempotency-key: choose-second"));
            }
            let body = reply.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: musicfs_session=fixture; Path=/api/v1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let session = ServerSession::login(&endpoint, "fixture").await.unwrap();
    let results = session
        .search_lyrics("track-a", "Song", "A, B", Some(3250))
        .await
        .unwrap();
    assert_eq!(results.items().len(), 2);
    assert_eq!(results.items()[1].lyrics, "Translation");
    for _ in 0..2 {
        results.apply(&session, 1, "choose-second").await.unwrap();
    }
    server.join().unwrap();
}
