use super::{Result, ServerSession};
use futures_util::{SinkExt, StreamExt};
use reqwest::{cookie::CookieStore, header};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};

pub(super) type ControlSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

impl ServerSession {
    pub(super) async fn control_socket(&self) -> Result<(ControlSocket, Value)> {
        self.ensure_active()?;
        let url = self.url("control");
        let cookie = self
            .cookies
            .cookies(&url)
            .ok_or("MusicFS authentication required (HTTP 401).")?;
        let mut ws_url = url;
        ws_url
            .set_scheme(if self.origin.scheme() == "https" {
                "wss"
            } else {
                "ws"
            })
            .map_err(|_| "Invalid MusicFS control URL.")?;
        let mut request = ws_url
            .as_str()
            .into_client_request()
            .map_err(|_| "Invalid MusicFS control request.")?;
        let headers = request.headers_mut();
        headers.insert(header::COOKIE, cookie);
        headers.insert(
            header::ORIGIN,
            self.info
                .endpoint
                .parse()
                .map_err(|_| "Invalid MusicFS origin.")?,
        );
        headers.insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            "musicfs.control.v1".parse().unwrap(),
        );
        let config = WebSocketConfig::default()
            .max_message_size(Some(2 * 1024 * 1024))
            .max_frame_size(Some(2 * 1024 * 1024));
        let (mut socket, handshake) =
            tokio_tungstenite::connect_async_with_config(request, Some(config), false)
                .await
                .map_err(|error| {
                    if let tokio_tungstenite::tungstenite::Error::Http(reply) = &error {
                        if reply.status() == reqwest::StatusCode::UNAUTHORIZED {
                            self.expired.store(true, Ordering::Release);
                            return "MusicFS authentication required (HTTP 401).";
                        }
                    }
                    "Unable to establish the MusicFS control channel."
                })?;
        if handshake
            .headers()
            .get(header::SEC_WEBSOCKET_PROTOCOL)
            .and_then(|v| v.to_str().ok())
            != Some("musicfs.control.v1")
        {
            return Err("Unexpected MusicFS control protocol.".into());
        }
        socket
            .send(Message::text(
                json!({"type":"auth","csrfToken":self.csrf}).to_string(),
            ))
            .await
            .map_err(|_| "MusicFS control authentication failed.")?;
        while let Some(message) = socket.next().await {
            let message = message.map_err(|_| "MusicFS control channel interrupted.")?;
            let Message::Text(text) = message else {
                continue;
            };
            let reply: Value = serde_json::from_str(text.as_str())
                .map_err(|_| "Invalid MusicFS control response.")?;
            if reply["type"] == "ready" {
                if reply["instanceId"].as_str() != Some(self.info.instance_id.as_str()) {
                    self.expired.store(true, Ordering::Release);
                    return Err("MusicFS server identity changed.".into());
                }
                self.ensure_active()?;
                return Ok((socket, reply));
            }
            return Err("Expected MusicFS control readiness.".into());
        }
        Err("MusicFS control connection closed during authentication.".into())
    }
}
