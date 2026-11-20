use crate::{check_id, decode, Page, Result, ServerSession};
use reqwest::Method;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "../../../tests/native/works.rs"]
mod tests;

#[derive(Deserialize)]
struct Items<T> {
    items: Vec<T>,
}

#[derive(Deserialize)]
struct ContentType {
    id: String,
    name: String,
    library: Option<Library>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Library {
    order: u32,
    icon: String,
    #[serde(default)]
    separate_history: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkType {
    id: String,
    name: String,
    icon: String,
    separate_history: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Credit {
    role: String,
    name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Work {
    work_id: String,
    content_type: String,
    title: String,
    description: String,
    credits: Vec<Credit>,
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkPage {
    items: Vec<Work>,
    next_after: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Package {
    work_id: String,
    package_id: String,
    name: String,
    kind: String,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    node_id: String,
    parent_id: String,
    name: String,
    path: String,
    kind: String,
    order: u32,
    source_track_id: String,
    track: Option<Source>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Source {
    source_track_id: String,
    title: Option<String>,
    artists: Option<Vec<String>>,
    duration_ms: Option<u64>,
    availability: String,
    album: Option<String>,
    cover_object_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioNode {
    id: String,
    group: String,
    directory: String,
    title: String,
    order: u32,
    artists: Vec<String>,
    duration_ms: Option<u64>,
    source_track_id: Option<String>,
    cover_object_id: Option<String>,
    album: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageAudio {
    id: String,
    name: String,
    kind: String,
    audio: Vec<AudioNode>,
}

#[derive(Serialize)]
pub struct WorkDetail {
    work: Work,
    packages: Vec<PackageAudio>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkHistoryEntry {
    work_id: String,
    work_title: String,
    source_track_id: String,
    track_title: String,
    recorded_at: String,
}

impl Package {
    fn audio(self) -> PackageAudio {
        let audio = self
            .nodes
            .into_iter()
            .filter(|n| n.kind == "audio")
            .map(|node| {
                let source = node.track;
                let playable = source.as_ref().filter(|t| {
                    t.availability == "available"
                        && t.source_track_id == node.source_track_id
                        && check_id(&t.source_track_id).is_ok()
                });
                AudioNode {
                    id: node.node_id,
                    group: node.parent_id,
                    directory: node
                        .path
                        .rsplit_once('/')
                        .map_or("", |(p, _)| p)
                        .to_string(),
                    source_track_id: playable.map(|t| t.source_track_id.clone()),
                    title: source
                        .as_ref()
                        .and_then(|t| t.title.clone())
                        .unwrap_or(node.name),
                    order: node.order,
                    artists: source
                        .as_ref()
                        .and_then(|t| t.artists.clone())
                        .unwrap_or_default(),
                    duration_ms: source.as_ref().and_then(|t| t.duration_ms),
                    album: source.as_ref().and_then(|t| t.album.clone()),
                    cover_object_id: source.and_then(|t| t.cover_object_id),
                }
            })
            .collect();
        PackageAudio {
            id: self.package_id,
            name: self.name,
            kind: self.kind,
            audio,
        }
    }
}

impl ServerSession {
    pub async fn content_types(&self) -> Result<Vec<WorkType>> {
        let items: Items<ContentType> = decode(
            self.send(Method::GET, self.url("content-types"), None)
                .await?,
        )
        .await?;
        let mut types: Vec<_> = items
            .items
            .into_iter()
            .filter_map(|t| {
                let library = t.library?;
                Some((
                    library.order,
                    WorkType {
                        id: t.id,
                        name: t.name,
                        icon: library.icon,
                        separate_history: library.separate_history,
                    },
                ))
            })
            .collect();
        types.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        Ok(types.into_iter().map(|(_, t)| t).collect())
    }

    pub async fn works(
        &self,
        content_type: &str,
        query: &str,
        cursor: Option<&str>,
    ) -> Result<Page<Work>> {
        let mut url = self.url("works");
        url.query_pairs_mut()
            .append_pair("contentType", check_id(content_type)?)
            .append_pair("limit", "40");
        if !query.trim().is_empty() {
            url.query_pairs_mut().append_pair("q", query.trim());
        }
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("after", cursor);
        }
        let page: WorkPage = decode(self.send(Method::GET, url, None).await?).await?;
        if page.items.iter().any(|w| w.content_type != content_type) {
            return Err("Unexpected work content type.".into());
        }
        Ok(Page {
            items: page.items,
            next_cursor: page.next_after,
        })
    }

    pub async fn work_detail(&self, work_id: &str) -> Result<WorkDetail> {
        let path = format!("works/{}", check_id(work_id)?);
        let work: Work = decode(self.send(Method::GET, self.url(&path), None).await?).await?;
        if work.work_id != work_id {
            return Err("Unexpected MusicFS work.".into());
        }
        let result: Items<Package> = decode(
            self.send(Method::GET, self.url(&format!("{path}/packages")), None)
                .await?,
        )
        .await?;
        if result.items.iter().any(|p| p.work_id != work_id) {
            return Err("Unexpected work package.".into());
        }
        Ok(WorkDetail {
            work,
            packages: result.items.into_iter().map(Package::audio).collect(),
        })
    }

    pub async fn work_history(&self, content_type: &str) -> Result<Vec<WorkHistoryEntry>> {
        let url = self.url(&format!("me/content-history/{}", check_id(content_type)?));
        let response: Items<WorkHistoryEntry> =
            decode(self.send(Method::GET, url, None).await?).await?;
        Ok(response.items)
    }
}
