use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio_tungstenite::tungstenite::Message;
use url::Url;
use uuid::Uuid;

use super::{check_id, Result, ServerSession, API_TIMEOUT};

#[derive(Clone)]
pub struct StreamLease {
    pub mime: String,
    pub codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub bitrate_bps: Option<u32>,
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
#[serde(rename_all = "camelCase")]
struct Profile {
    mime: String,
    transport: String,
    codec: Option<String>,
    sample_rate: Option<u32>,
    bitrate_bps: Option<u32>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeekRange {
    start_ms: u64,
    end_ms: u64,
}
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
    // 播放命令只发送一次，断线后不重试结果未知的操作。
    async fn command(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let id = Uuid::new_v4().to_string();
        let mut frame = json!({ "type":"request", "id":id, "method":method, "path":path });
        if let Some(body) = body {
            frame["body"] = body;
            frame["idempotencyKey"] = json!(Uuid::new_v4().to_string());
        }
        let run = async {
            let (mut socket, _) = self.control_socket().await?;
            socket
                .send(Message::text(frame.to_string()))
                .await
                .map_err(|_| "MusicFS control request interrupted.")?;
            while let Some(message) = socket.next().await {
                let Message::Text(text) =
                    message.map_err(|_| "MusicFS control channel interrupted.")?
                else {
                    continue;
                };
                let reply: Value = serde_json::from_str(text.as_str())
                    .map_err(|_| "Invalid MusicFS control response.")?;
                if reply["type"] == "response" && reply["id"].as_str() == Some(id.as_str()) {
                    let status = reply["status"]
                        .as_u64()
                        .ok_or("Invalid MusicFS control status.")?;
                    let _ = socket.close(None).await;
                    if !(200..300).contains(&status) {
                        let code = reply
                            .pointer("/body/error/code")
                            .and_then(Value::as_str)
                            .filter(|s| {
                                s.len() < 64
                                    && s.bytes().all(|c| c.is_ascii_uppercase() || c == b'_')
                            })
                            .unwrap_or("CONTROL_ERROR");
                        return Err(format!("MusicFS returned {code} (HTTP {status})."));
                    }
                    return Ok(reply["body"].clone());
                }
            }
            Err("MusicFS control connection closed before the operation was confirmed.".into())
        };
        tokio::time::timeout(API_TIMEOUT, run)
            .await
            .map_err(|_| "MusicFS control operation timed out.")?
    }

    pub async fn create_playback(
        &self,
        track_id: &str,
        source_track_id: &str,
        forced_source_track_id: Option<&str>,
        mimes: &[&str],
        force_processing: bool,
    ) -> Result<StreamLease> {
        check_id(source_track_id)?;
        if let Some(id) = forced_source_track_id {
            check_id(id)?;
        }
        let source = if track_id.is_empty() {
            if forced_source_track_id.is_some() {
                return Err("Unexpected forced source without a library track.".into());
            }
            source_track_id.to_owned()
        } else {
            self.playback_source(track_id, forced_source_track_id)
                .await?
        };
        let body = create_body(
            track_id,
            &source,
            forced_source_track_id,
            mimes,
            force_processing,
        );
        let reply = self
            .command("POST", "/playback/sessions", Some(body))
            .await?;
        let id = check_id(
            reply["sessionId"]
                .as_str()
                .ok_or("Missing MusicFS playback session.")?,
        )?
        .to_owned();
        let result = (|| {
            let d: Descriptor = serde_json::from_value(reply)
                .map_err(|_| "Invalid MusicFS playback descriptor.")?;
            check_id(&d.attempt_id)?;
            let url = self.url(&format!(
                "playback/sessions/{id}/attempts/{}/stream",
                d.attempt_id
            ));
            if d.source_track_id != source
                || d.session_id != id
                || d.stream_url != url.path()
                || !matches!(d.delivery_mode.as_str(), "proxy" | "processed")
                || !matches!(d.seek_mode.as_str(), "byteRange" | "timeRestart" | "none")
                || d.seekable_range
                    .as_ref()
                    .is_some_and(|r| r.start_ms > r.end_ms)
                || d.output_profile.transport != "nativeHttp"
                || !mimes
                    .iter()
                    .any(|mime| mime.split(';').next() == Some(d.output_profile.mime.as_str()))
            {
                return Err("Unsupported MusicFS playback descriptor.".into());
            }
            Ok(StreamLease {
                id: id.clone(),
                attempt_id: d.attempt_id,
                source_id: d.source_track_id,
                mime: d.output_profile.mime,
                codec: d.output_profile.codec,
                sample_rate: d.output_profile.sample_rate,
                bitrate_bps: d.output_profile.bitrate_bps,
                url,
                byte_range: d.seek_mode == "byteRange",
                seek_range_ms: d.seekable_range.map(|r| (r.start_ms, r.end_ms)),
                owner: self.active.clone(),
                closed: Arc::new(AtomicBool::new(false)),
            })
        })();
        if result.is_err() {
            let _ = self
                .command("DELETE", &format!("/playback/sessions/{id}"), None)
                .await;
        }
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
        self.send(Method::POST, self.url("me/recently-played"), Some(&body))
            .await
            .map(|_| ())
    }

    pub async fn close_playback(&self, lease: &StreamLease) -> Result<()> {
        if !Arc::ptr_eq(&self.active, &lease.owner) {
            return Err("Mismatched MusicFS playback session.".into());
        }
        lease.closed.store(true, Ordering::Release);
        self.command("DELETE", &format!("/playback/sessions/{}", lease.id), None)
            .await
            .map(|_| ())
    }
}

fn create_body(
    track_id: &str,
    source_id: &str,
    forced_source_id: Option<&str>,
    mimes: &[&str],
    force_processing: bool,
) -> Value {
    let formats: Vec<Value> = mimes
        .iter()
        .map(|mime| json!({"mime":mime, "canPlay":"probably"}))
        .collect();
    let mut body = json!({ "sourceTrackId":source_id, "capabilities":{"nativeHttp":true,"formats":formats},
        "preferences":{"dspExecution":"client","forceProcessing":force_processing,"gainDb":0} });
    if !track_id.is_empty() {
        body["trackId"] = json!(track_id);
    }
    if let Some(id) = forced_source_id {
        body["forcedSourceTrackId"] = json!(id);
    }
    body
}

#[cfg(test)]
mod tests {
    #[test]
    fn playback_profile_retains_negotiated_codec_and_optional_bitrate() {
        let p: super::Profile = serde_json::from_value(serde_json::json!({
            "mime": "audio/mpeg", "codec": "mp3", "transport": "nativeHttp", "sampleRate": 44100, "bitrateBps": 320000
        }))
        .unwrap();
        assert_eq!(p.codec.as_deref(), Some("mp3"));
        assert_eq!(p.sample_rate, Some(44100));
        assert_eq!(p.bitrate_bps, Some(320000));
        let p: super::Profile = serde_json::from_value(serde_json::json!({
            "mime": "audio/flac", "transport": "nativeHttp", "codec": null
        }))
        .unwrap();
        assert!(p.codec.is_none() && p.bitrate_bps.is_none());
    }

    #[test]
    fn work_track_uses_only_its_source_identity() {
        let body = super::create_body("", "work-audio", None, &["audio/wav"], true);
        assert_eq!(body["preferences"]["forceProcessing"], true);
        assert_eq!(body["preferences"]["dspExecution"], "client");
        assert_eq!(body["sourceTrackId"], "work-audio");
        assert!(body.get("trackId").is_none());
        assert!(body.get("forcedSourceTrackId").is_none());
    }
}
