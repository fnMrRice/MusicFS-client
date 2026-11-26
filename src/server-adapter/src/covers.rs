use super::{check_id, limited, Result, ServerSession, API_TIMEOUT};
use reqwest::{header, Client, Method};
use std::time::Duration;
use url::{Host, Url};

pub(super) fn image_client() -> Result<Client> {
    Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(API_TIMEOUT)
        .build()
        .map_err(|_| "Could not initialize the public image client.".into())
}

pub(super) fn cover_endpoint(cover: &str) -> Result<String> {
    if let Some(id) = cover
        .strip_prefix("/api/v1/source-tracks/")
        .and_then(|s| s.strip_suffix("/online-artwork"))
    {
        return Ok(format!("source-tracks/{}/online-artwork", check_id(id)?));
    }
    Ok(format!("media/{}", media_object_id(cover)?))
}

pub(super) fn media_object_id(cover: &str) -> Result<&str> {
    check_id(cover.strip_prefix("/api/v1/media/").unwrap_or(cover))
}

fn public_cover(raw: &str) -> Result<Url> {
    let invalid = || "Invalid public cover URL.".to_owned();
    let url = Url::parse(raw).map_err(|_| invalid())?;
    if raw.len() > 4096
        || url.scheme() != "https"
        || !matches!(url.host(), Some(Host::Domain(name)) if name.contains('.') && !name.ends_with(".localhost"))
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path().len() < 2
    {
        return Err(invalid());
    }
    Ok(url)
}

impl ServerSession {
    pub async fn cover_bytes(&self, cover: &str) -> Result<Vec<u8>> {
        self.ensure_active()?;
        let response = if cover.starts_with("https://") {
            // 在线插件给出已校验的公共图片地址。独立客户端与 MusicFS 登录会话隔离。
            let response = self
                .image_client
                .get(public_cover(cover)?)
                .send()
                .await
                .map_err(|_| "Could not load the online cover.".to_owned())?;
            if !response.status().is_success() {
                return Err("Online cover is unavailable.".into());
            }
            response
        } else {
            self.send(Method::GET, self.url(&cover_endpoint(cover)?), None)
                .await?
        };
        let mime = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        if !matches!(
            mime,
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) {
            return Err("Unsupported cover image format.".into());
        }
        let bytes = limited(response, 8 * 1024 * 1024).await?;
        self.ensure_active()?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_covers_use_display_urls_without_credentials_or_query_parameters() {
        assert!(public_cover("https://p2.music.126.net/cover.jpg").is_ok());
        for invalid in [
            "http://p1.music.126.net/cover.jpg",
            "https://user:pass@p1.music.126.net/cover.jpg",
            "https://p1.music.126.net/cover.jpg?token=x",
            "https://127.0.0.1/cover.jpg",
            "https://localhost/cover.jpg",
        ] {
            assert!(public_cover(invalid).is_err());
        }
    }
}
