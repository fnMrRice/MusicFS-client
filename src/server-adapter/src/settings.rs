use super::{limited, Result, ServerSession};
use reqwest::Method;
use serde_json::Value;
use url::Url;

// 设置页面只访问当前会话的 API，查询参数由各功能按自身协议编码。
fn api_url(origin: &Url, path: &str) -> Result<Url> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains(['#', '\\']) {
        return Err("Invalid MusicFS API path.".into());
    }
    let url = origin
        .join(&format!("/api/v1{path}"))
        .map_err(|_| "Invalid MusicFS API path.")?;
    if url.origin() != origin.origin() || !url.path().starts_with("/api/v1/") {
        return Err("Invalid MusicFS API path.".into());
    }
    Ok(url)
}

impl ServerSession {
    pub async fn settings_json(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        headers: &[(&str, &str)],
    ) -> Result<Value> {
        let method = Method::from_bytes(method.as_bytes()).map_err(|_| "Invalid HTTP method.")?;
        let response = self
            .send_with_headers(method, api_url(&self.origin, path)?, body, headers)
            .await?;
        let bytes = limited(response, 16 * 1024 * 1024).await?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| "Invalid MusicFS response.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_query_encoding_and_api_scope() {
        let origin = Url::parse("https://music.example/").unwrap();
        let url = api_url(&origin, "/library/tracks?q=A%2FB%20C").unwrap();
        assert_eq!(url.query_pairs().next().unwrap().1, "A/B C");
        for path in ["//another.example/", "/../../elsewhere", "/%2e%2e/secret"] {
            assert!(api_url(&origin, path).is_err());
        }
    }
}
