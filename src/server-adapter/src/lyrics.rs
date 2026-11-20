use reqwest::{header, Method};
use serde::Deserialize;

use super::{check_id, decode, limited, Result, ServerSession};

#[derive(Deserialize)]
struct Selection { bindings: Vec<Binding> }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Binding { field: String, state: String, current: bool, object_id: String }

impl ServerSession {
    pub async fn lyrics(&self, source_track_id: &str) -> Result<Option<String>> {
        let id = check_id(source_track_id)?;
        let url = self.url(&format!("source-tracks/{id}/media-selection"));
        let selection: Selection = decode(self.send(Method::GET, url, None).await?).await?;
        let Some(binding) = selection.bindings.into_iter().find(|b| b.current && b.state == "available" && b.field == "lyrics") else {
            return Ok(None);
        };
        let url = self.url(&format!("media/{}", check_id(&binding.object_id)?));
        let response = self.send(Method::GET, url, None).await?;
        let mime = response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).and_then(|v| v.split(';').next());
        if !mime.is_some_and(|s| s.trim().eq_ignore_ascii_case("text/plain")) { return Err("Unsupported lyrics format.".into()); }
        let lyrics = String::from_utf8(limited(response, 1024 * 1024).await?).map_err(|_| "Lyrics are not valid UTF-8.")?;
        Ok((!lyrics.trim().is_empty()).then_some(lyrics))
    }
}
