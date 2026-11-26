use super::*;
use serde_json::json;

#[test]
fn online_queue_uses_resolved_metadata_and_falls_back_to_listing_art() {
    let fallback = serde_json::from_value::<TrackInfo>(json!({
        "externalId": "item", "title": "Listing title", "artists": ["Listing artist"],
        "coverUrl": "https://music.example/cover.jpg"
    }))
    .unwrap();
    let mut response = json!({
        "providerInstanceId": "provider", "assetId": "asset", "availability": "available",
        "sourceTrackId": "playable"
    });
    let queue = online_queue_track(
        serde_json::from_value(response.clone()).unwrap(),
        fallback.clone(),
    );
    assert_eq!(queue.cover(), fallback.cover_url.as_deref());
    assert_eq!(queue.title, fallback.title);
    assert!(queue.source_only);
    assert_eq!(queue.source_track_id.as_deref(), Some("playable"));

    let path = "/api/v1/source-tracks/playable/online-artwork";
    response["onlineCoverPath"] = json!(path);
    response["title"] = json!("Resolved title");
    let queue = online_queue_track(
        serde_json::from_value(response.clone()).unwrap(),
        fallback.clone(),
    );
    assert_eq!(queue.cover(), Some(path));
    assert_eq!(queue.title, "Resolved title");
    response["coverObjectId"] = json!("cover");
    let queue = online_queue_track(serde_json::from_value(response).unwrap(), fallback);
    assert_eq!(queue.cover(), Some("cover"));
}
