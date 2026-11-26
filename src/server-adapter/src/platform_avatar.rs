use crate::{limited, Result, ServerSession};
use reqwest::Url;

// 与网易云插件的头像验证保持一致。外部图片使用独立客户端，绝不携带 MusicFS 会话。
fn avatar_url(raw: &str) -> Result<Url> {
    let u = Url::parse(raw).map_err(|_| "Invalid platform avatar URL.")?;
    if raw.len() > 2048
        || u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path().len() < 2
        || !matches!(
            u.host_str(),
            Some("p1.music.126.net" | "p2.music.126.net" | "p3.music.126.net" | "p4.music.126.net")
        )
    {
        return Err("Unsupported platform avatar URL.".into());
    }
    Ok(u)
}

impl ServerSession {
    pub async fn platform_avatar(&self, raw: &str) -> Result<Vec<u8>> {
        self.ensure_active()?;
        let url = avatar_url(raw)?;
        let reply = self
            .image_client
            .get(url)
            .send()
            .await
            .map_err(|_| "Could not load platform avatar.")?;
        if !reply.status().is_success() {
            return Err("Platform avatar is unavailable.".into());
        }
        let bytes = limited(reply, 2 * 1024 * 1024).await?;
        self.ensure_active()?;
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn avatar_origin_matches_the_source_plugin() {
        assert!(avatar_url("https://p2.music.126.net/avatar.jpg").is_ok());
        for invalid in [
            "https://p2.music.126.net.evil.test/avatar",
            "https://user@p2.music.126.net/avatar",
            "https://p2.music.126.net/avatar?token=t",
            "http://p2.music.126.net/avatar",
        ] {
            assert!(avatar_url(invalid).is_err());
        }
    }
}
