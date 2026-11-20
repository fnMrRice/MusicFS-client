use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use futures_util::{SinkExt, StreamExt};
use reqwest::{cookie::CookieStore, header, Method};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message};
use url::Url;
use uuid::Uuid;

use super::{check_id, Result, ServerSession, API_TIMEOUT};

#[derive(Clone)]
pub struct StreamLease {
    pub mime: String,
    pub source_id: String,
    pub seek_range_ms: Option<(u64, u64)>,
    pub(crate) url: Url,
    pub(crate) byte_range: bool,
    pub(crate) closed: Arc<AtomicBool>,
    owner: Arc<AtomicBool>,
    id: String,
    attempt_id: String,
}

#[derive(Deserialize)]
struct Profile { mime: String, transport: String }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeekRange { start_ms: u64, end_ms: u64 }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Descriptor {
    session_id: String,
    attempt_id: String,
    source_track_id: String,
    stream_url: String,
    delivery_mode: String,
    seek_mode: String,
    seekable_range: Option<SeekRange>,
    output_profile: Profile,
}

impl ServerSession {
    // 每次命令使用独立短连接，仅在收到同一实例的 ready 后发送，不重试结果未知的写操作。
    async fn command(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        self.ensure_active()?;
        let url = self.url("control");
        let cookie = self.cookies.cookies(&url).ok_or("MusicFS authentication required (HTTP 401).")?;
        let mut ws_url = url;
        ws_url.set_scheme(if self.origin.scheme() == "https" { "wss" } else { "ws" }).map_err(|_| "Invalid MusicFS control URL.")?;
        let mut request = ws_url.as_str().into_client_request().map_err(|_| "Invalid MusicFS control request.")?;
        let headers = request.headers_mut();
        headers.insert(header::COOKIE, cookie);
        headers.insert(header::ORIGIN, self.info.endpoint.parse().map_err(|_| "Invalid MusicFS origin.")?);
        headers.insert(header::SEC_WEBSOCKET_PROTOCOL, "musicfs.control.v1".parse().unwrap());
        let id = Uuid::new_v4().to_string();
        let mut frame = json!({ "type":"request", "id":id, "method":method, "path":path });
        if let Some(body) = body {
            frame["body"] = body;
            frame["idempotencyKey"] = json!(Uuid::new_v4().to_string());
        }
        let run = async {
            let config = WebSocketConfig::default().max_message_size(Some(2 * 1024 * 1024)).max_frame_size(Some(2 * 1024 * 1024));
            let (mut socket, handshake) = tokio_tungstenite::connect_async_with_config(request, Some(config), false).await
                .map_err(|_| "Unable to establish the MusicFS control channel.")?;
            if handshake.headers().get(header::SEC_WEBSOCKET_PROTOCOL).and_then(|v| v.to_str().ok()) != Some("musicfs.control.v1") {
                return Err("Unexpected MusicFS control protocol.".into());
            }
            socket.send(Message::text(json!({"type":"auth","csrfToken":self.csrf}).to_string())).await
                .map_err(|_| "MusicFS control authentication failed.")?;
            let mut sent = false;
            while let Some(message) = socket.next().await {
                let message = message.map_err(|_| "MusicFS control channel interrupted.")?;
                let Message::Text(text) = message else { continue; };
                let reply: Value = serde_json::from_str(text.as_str()).map_err(|_| "Invalid MusicFS control response.")?;
                match reply["type"].as_str() {
                    Some("ready") if !sent => {
                        if reply["instanceId"].as_str() != Some(self.info.instance_id.as_str()) {
                            return Err("MusicFS server identity changed.".into());
                        }
                        self.ensure_active()?;
                        socket.send(Message::text(frame.to_string())).await.map_err(|_| "MusicFS control request interrupted.")?;
                        sent = true;
                    }
                    Some("response") if sent && reply["id"].as_str() == Some(id.as_str()) => {
                        let status = reply["status"].as_u64().ok_or("Invalid MusicFS control status.")?;
                        let _ = socket.close(None).await;
                        if !(200..300).contains(&status) {
                            let code = reply.pointer("/body/error/code").and_then(Value::as_str).filter(|s|
                                s.len() < 64 && s.bytes().all(|c| c.is_ascii_uppercase() || c == b'_')).unwrap_or("CONTROL_ERROR");
                            return Err(format!("MusicFS returned {code} (HTTP {status})."));
                        }
                        return Ok(reply["body"].clone());
                    }
                    _ => {}
                }
            }
            Err("MusicFS control connection closed before the operation was confirmed.".into())
        };
        tokio::time::timeout(API_TIMEOUT, run).await.map_err(|_| "MusicFS control operation timed out.")?
    }

    pub async fn create_playback(&self, track_id: &str, source_track_id: &str, forced_source_track_id: Option<&str>, mimes: &[&str]) -> Result<StreamLease> {
        check_id(source_track_id)?;
        if let Some(id) = forced_source_track_id { check_id(id)?; }
        let source = self.playback_source(track_id, forced_source_track_id).await?;
        let formats: Vec<Value> = mimes.iter().map(|mime| json!({"mime":mime, "canPlay":"probably"})).collect();
        let mut body = json!({ "trackId":track_id, "sourceTrackId":source,
            "capabilities":{"nativeHttp":true,"formats":formats}, "preferences":{"dspExecution":"server"} });
        if let Some(id) = forced_source_track_id { body["forcedSourceTrackId"] = json!(id); }
        let reply = self.command("POST", "/playback/sessions", Some(body)).await?;
        let id = check_id(reply["sessionId"].as_str().ok_or("Missing MusicFS playback session.")?)?.to_owned();
        let result = (|| {
            let d: Descriptor = serde_json::from_value(reply).map_err(|_| "Invalid MusicFS playback descriptor.")?;
            check_id(&d.attempt_id)?;
            let url = self.url(&format!("playback/sessions/{id}/attempts/{}/stream", d.attempt_id));
            if d.source_track_id != source || d.session_id != id || d.stream_url != url.path()
                || !matches!(d.delivery_mode.as_str(), "proxy" | "processed") || !matches!(d.seek_mode.as_str(), "byteRange" | "timeRestart" | "none")
                || d.seekable_range.as_ref().is_some_and(|r| r.start_ms > r.end_ms)
                || d.output_profile.transport != "nativeHttp" || !mimes.iter().any(|mime| mime.split(';').next() == Some(d.output_profile.mime.as_str())) {
                return Err("Unsupported MusicFS playback descriptor.".into());
            }
            Ok(StreamLease { id: id.clone(), attempt_id: d.attempt_id, source_id: d.source_track_id,
                mime: d.output_profile.mime, url, byte_range: d.seek_mode == "byteRange",
                seek_range_ms: d.seekable_range.map(|r| (r.start_ms, r.end_ms)),
                owner: self.active.clone(), closed: Arc::new(AtomicBool::new(false)) })
        })();
        if result.is_err() { let _ = self.command("DELETE", &format!("/playback/sessions/{id}"), None).await; }
        result
    }

    pub(crate) fn check_stream(&self, lease: &StreamLease) -> Result<()> {
        self.ensure_active()?;
        if !Arc::ptr_eq(&self.active, &lease.owner) || lease.closed.load(Ordering::Acquire) {
            return Err("MusicFS playback session closed.".into());
        }
        Ok(())
    }

    pub async fn record_started(&self, lease: &StreamLease) -> Result<()> {
        self.check_stream(lease)?;
        let body = json!({ "sourceTrackId":lease.source_id, "playbackSessionId":lease.id, "attemptId":lease.attempt_id });
        self.send(Method::POST, self.url("me/recently-played"), Some(&body)).await.map(|_| ())
    }

    pub async fn close_playback(&self, lease: &StreamLease) -> Result<()> {
        if !Arc::ptr_eq(&self.active, &lease.owner) { return Err("Mismatched MusicFS playback session.".into()); }
        lease.closed.store(true, Ordering::Release);
        self.command("DELETE", &format!("/playback/sessions/{}", lease.id), None).await.map(|_| ())
    }
}
