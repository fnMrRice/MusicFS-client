mod catalog;
pub mod folders;
mod lyrics;
mod playback;
mod playlist_edit;
mod reader;
mod recommendations;
mod settings;
mod track;
mod track_actions;
pub use track::{SourceGroup, SourceVersion};
mod works;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use reqwest::{
    cookie::{CookieStore, Jar},
    header, Client, Method, Response, StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use url::{Host, Url};

pub use catalog::{Category, CategoryKind, Page, Playlist, Track};
pub use playback::StreamLease;
pub use playlist_edit::PlaylistChange;
pub use reader::StreamReader;
pub use recommendations::{RecommendationFeed, RecommendedPlaylistDetail};
pub use works::{AudioNode, PackageAudio, Work, WorkDetail, WorkHistoryEntry, WorkType};

type Result<T> = std::result::Result<T, String>;
const API_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub endpoint: String,
    pub user_name: String,
    pub instance_id: String,
    pub server_version: String,
    pub role: String,
}

#[derive(Clone)]
pub struct ServerSession {
    pub info: SessionInfo,
    origin: Url,
    client: Client,
    cookies: Arc<Jar>,
    csrf: String,
    active: Arc<AtomicBool>,
    expired: Arc<AtomicBool>,
    runtime: tokio::runtime::Handle,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ready {
    product: String,
    api_version: String,
    instance_id: String,
    server_version: String,
    ready: bool,
}
#[derive(Deserialize)]
struct User {
    id: String,
    role: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Login {
    user: User,
    csrf_token: String,
}

fn origin(address: &str) -> Result<Url> {
    let mut url = Url::parse(address.trim()).map_err(|_| "Enter a valid MusicFS server URL.")?;
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        Some(Host::Domain("localhost")) => true,
        _ => false,
    };
    if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path().trim_end_matches('/'), "" | "/api/v1")
    {
        return Err("Use HTTPS or a loopback HTTP MusicFS server address.".into());
    }
    url.set_path("/");
    Ok(url)
}

fn check_id(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("Invalid MusicFS object identifier.".into());
    }
    Ok(value)
}

async fn limited(mut response: Response, max: usize) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|n| n > max as u64) {
        return Err("MusicFS response is too large.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Failed to read the MusicFS response.")?
    {
        if chunk.len() > max - bytes.len() {
            return Err("MusicFS response is too large.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn checked(response: Response) -> Result<Response> {
    let code = response.status();
    if code.is_success() {
        return Ok(response);
    }
    if code == StatusCode::UNAUTHORIZED {
        return Err("MusicFS authentication required (HTTP 401).".into());
    }
    let data = limited(response, 8192).await.ok();
    let json = data.and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let error = json
        .as_ref()
        .and_then(|v| v.pointer("/error/code"))
        .and_then(Value::as_str)
        .filter(|code| code.len() < 64 && code.bytes().all(|c| c.is_ascii_uppercase() || c == b'_'))
        .unwrap_or("HTTP_ERROR");
    Err(format!(
        "MusicFS returned {error} (HTTP {}).",
        code.as_u16()
    ))
}

async fn decode<T: DeserializeOwned>(response: Response) -> Result<T> {
    let bytes = limited(checked(response).await?, 16 * 1024 * 1024).await?;
    serde_json::from_slice(&bytes).map_err(|_| "Invalid MusicFS response.".into())
}

impl ServerSession {
    pub async fn login(server_url: &str, token: &str) -> Result<Self> {
        Self::login_pinned(server_url, token, None).await
    }

    pub async fn login_pinned(
        server_url: &str,
        token: &str,
        expected_instance: Option<&str>,
    ) -> Result<Self> {
        let origin = origin(server_url)?;
        if token.is_empty() || token.len() > 4096 || !token.bytes().all(|c| c.is_ascii_graphic()) {
            return Err("Enter a valid MusicFS access token.".into());
        }
        let cookies = Arc::new(Jar::default());
        let client = Client::builder()
            .cookie_provider(cookies.clone())
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .read_timeout(API_TIMEOUT)
            .build()
            .map_err(|_| "Unable to initialize the MusicFS connection.")?;
        let endpoint = origin.origin().ascii_serialization();
        let ready: Ready = decode(
            client
                .get(format!("{endpoint}/api/v1/health/ready"))
                .timeout(API_TIMEOUT)
                .send()
                .await
                .map_err(|_| "Unable to reach the MusicFS server.")?,
        )
        .await?;
        if !ready.ready
            || ready.product != "MusicFS"
            || ready.api_version != "1"
            || ready.instance_id.is_empty()
        {
            return Err("This endpoint is not a ready MusicFS v1 server.".into());
        }
        if expected_instance.is_some_and(|expected| expected != ready.instance_id) {
            return Err("The saved MusicFS server identity has changed. Sign in manually.".into());
        }
        let auth = origin
            .join("api/v1/auth/session")
            .map_err(|_| "Invalid MusicFS URL.")?;
        let login: Login = decode(
            client
                .post(auth.clone())
                .header(header::ORIGIN, &endpoint)
                .bearer_auth(token)
                .timeout(API_TIMEOUT)
                .send()
                .await
                .map_err(|_| "Unable to sign in to MusicFS.")?,
        )
        .await?;
        let session_cookie = cookies
            .cookies(&auth)
            .and_then(|v| v.to_str().ok().map(str::to_owned))
            .is_some_and(|v| {
                v.split(';').any(|pair| {
                    pair.trim()
                        .strip_prefix("musicfs_session=")
                        .is_some_and(|id| !id.is_empty())
                })
            });
        if !session_cookie || login.csrf_token.is_empty() || login.user.id.is_empty() {
            return Err("MusicFS did not establish a valid session.".into());
        }
        Ok(Self {
            info: SessionInfo {
                endpoint,
                user_name: login.user.id,
                role: login.user.role,
                instance_id: ready.instance_id,
                server_version: ready.server_version,
            },
            origin,
            client,
            cookies,
            csrf: login.csrf_token,
            active: Arc::new(AtomicBool::new(true)),
            expired: Arc::default(),
            runtime: tokio::runtime::Handle::current(),
        })
    }

    fn ensure_active(&self) -> Result<()> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err("MusicFS session closed.".into())
        }
    }

    pub fn expired(&self) -> bool {
        self.expired.load(Ordering::Acquire)
            || self.cookies.cookies(&self.url("auth/session")).is_none()
    }

    fn url(&self, path: &str) -> Url {
        let mut url = self.origin.clone();
        url.set_path(&format!("/api/v1/{path}"));
        url
    }

    async fn send(&self, method: Method, url: Url, body: Option<&Value>) -> Result<Response> {
        self.send_with_headers(method, url, body, &[]).await
    }

    async fn send_with_headers(
        &self,
        method: Method,
        url: Url,
        body: Option<&Value>,
        headers: &[(&str, &str)],
    ) -> Result<Response> {
        self.ensure_active()?;
        let mut request = self
            .client
            .request(method.clone(), url)
            .timeout(API_TIMEOUT);
        if method != Method::GET {
            request = request
                .header(header::ORIGIN, &self.info.endpoint)
                .header("X-CSRF-Token", &self.csrf);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        for &(name, value) in headers {
            if !["Idempotency-Key", "If-Match", "If-None-Match"]
                .iter()
                .any(|allowed| name.eq_ignore_ascii_case(allowed))
            {
                return Err("Unsupported MusicFS request header.".into());
            }
            request = request.header(name, value);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "Unable to reach the MusicFS server.")?;
        self.ensure_active()?;
        if response.status() == StatusCode::UNAUTHORIZED {
            self.expired.store(true, Ordering::Release);
        }
        checked(response).await
    }

    pub async fn logout(&self) -> Result<()> {
        if !self.active.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        let reply = self
            .client
            .delete(self.url("auth/session"))
            .header(header::ORIGIN, &self.info.endpoint)
            .header("X-CSRF-Token", &self.csrf)
            .timeout(API_TIMEOUT)
            .send()
            .await
            .map_err(|_| "Unable to revoke the MusicFS session.")?;
        if reply.status() == StatusCode::UNAUTHORIZED {
            return Ok(());
        }
        checked(reply).await.map(|_| ())
    }

    pub async fn cover_bytes(&self, id: &str) -> Result<Vec<u8>> {
        let url = self.url(&format!("media/{}", check_id(id)?));
        let response = self.send(Method::GET, url, None).await?;
        let mime = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_owned();
        if !matches!(
            mime.as_str(),
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) {
            return Err("Unsupported cover image format.".into());
        }
        limited(response, 8 * 1024 * 1024).await
    }
}
