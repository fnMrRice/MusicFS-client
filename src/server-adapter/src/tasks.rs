use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPage {
    pub items: Vec<LibraryTask>,
    pub active_count: usize,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTask {
    pub id: String,
    pub owner_name: String,
    pub status: String,
    pub cancellable: bool,
    pub error: Option<TaskError>,
    #[serde(flatten)]
    pub details: TaskDetails,
}

#[derive(Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskDetails {
    MetadataRefresh {
        provider_name: String,
        roots: Vec<Root>,
    },
    AlbumAutoScrape {
        progress: Progress,
        result: AlbumResult,
    },
    BulkMerge {
        result: MergeResult,
    },
}

#[derive(Clone, Deserialize)]
pub struct Progress {
    pub completed: usize,
    pub total: usize,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumResult {
    pub updated: usize,
    pub retained: usize,
    pub not_found: usize,
    pub failed: usize,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub merged_groups: usize,
    pub merged_sources: usize,
    pub skipped_groups: usize,
}

#[derive(Clone, Deserialize)]
pub struct TaskError {
    pub message: String,
}

#[derive(Clone, Deserialize)]
pub struct Root {
    pub name: String,
}

#[derive(Clone, Deserialize)]
pub struct TaskSnapshot {
    pub page: TaskPage,
    pub changed: Vec<LibraryTask>,
    pub reset: bool,
}

type Update = (u64, Option<Result<TaskSnapshot, String>>);

pub struct TaskUpdates {
    rx: watch::Receiver<Update>,
    sequence: u64,
}

impl TaskUpdates {
    pub fn take(&mut self) -> Option<Result<TaskSnapshot, String>> {
        if !self.rx.has_changed().unwrap_or(false) {
            return None;
        }
        let (sequence, result) = self.rx.borrow_and_update().clone();
        let mut result = result?;
        if let Ok(snapshot) = &mut result {
            // 窗口挂起期间可能错过首屏之外的增量，恢复时核对已打开的历史页。
            snapshot.reset |= sequence != self.sequence + 1;
        }
        self.sequence = sequence;
        Some(result)
    }
}

#[derive(Clone)]
pub(super) struct Subscription {
    all: bool,
    tx: watch::Sender<Update>,
}

impl Subscription {
    pub fn new(all: bool) -> (Self, TaskUpdates) {
        let (tx, rx) = watch::channel((0, None));
        (Self { all, tx }, TaskUpdates { rx, sequence: 0 })
    }

    pub fn request(&self) -> (String, Message) {
        let id = Uuid::new_v4().to_string();
        let frame = json!({"type":"request","id":id,"method":"POST","path":"/tasks/subscription",
            "body":{"scope":if self.all { "all" } else { "mine" },"subscriptionId":id}});
        (id, Message::text(frame.to_string()))
    }

    pub fn receive(&self, id: &str, frame: &Value) -> bool {
        let result = match frame["type"].as_str() {
            Some("response") if frame["id"] == id && frame["status"] != 204 => Err(format!(
                "MusicFS task subscription failed (HTTP {}).",
                frame["status"]
            )),
            Some("event")
                if frame["event"] == "tasks.snapshot" && frame["data"]["subscriptionId"] == id =>
            {
                serde_json::from_value(frame["data"].clone())
                    .map_err(|_| "Invalid MusicFS task snapshot.".to_owned())
            }
            _ => return false,
        };
        self.tx.send_modify(|(sequence, update)| {
            *sequence += 1;
            *update = Some(result);
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_library_task_results_without_refresh_roots() {
        let common = json!({"id":"id","ownerName":"user","status":"succeeded","cancellable":false});
        for (kind, result) in [
            (
                "albumAutoScrape",
                json!({"updated":2,"retained":1,"notFound":1,"failed":0}),
            ),
            (
                "bulkMerge",
                json!({"mergedGroups":2,"mergedSources":4,"skippedGroups":1}),
            ),
        ] {
            let mut row = common.clone();
            row["kind"] = json!(kind);
            row["result"] = result;
            row["progress"] = json!({"total":4,"completed":4});
            let task: LibraryTask = serde_json::from_value(row).unwrap();
            match task.details {
                TaskDetails::AlbumAutoScrape { progress, result } => {
                    assert_eq!(
                        (progress.total, result.updated, result.not_found),
                        (4, 2, 1)
                    );
                }
                TaskDetails::BulkMerge { result } => assert_eq!(result.merged_sources, 4),
                TaskDetails::MetadataRefresh { .. } => panic!("expected a library task"),
            }
        }
    }

    #[test]
    fn scopes_requests_and_recovers_missed_history_changes() {
        let (subscription, mut changes) = Subscription::new(true);
        let (id, request) = subscription.request();
        let body: Value = serde_json::from_str(request.to_text().unwrap()).unwrap();
        assert_eq!(body["body"]["scope"], "all");
        let snapshot = json!({"type":"event","event":"tasks.snapshot","data":{
            "subscriptionId":id,"page":{"items":[],"nextCursor":null,"activeCount":0},
            "reset":false,"changed":[]}});
        assert!(!subscription.receive("old-subscription", &snapshot));
        assert!(changes.take().is_none());
        assert!(subscription.receive(&id, &snapshot));
        assert!(!changes.take().unwrap().unwrap().reset);
        assert!(subscription.receive(&id, &snapshot));
        assert!(subscription.receive(&id, &snapshot));
        assert!(changes.take().unwrap().unwrap().reset);
        assert!(subscription.receive(&id, &json!({"type":"response","id":id,"status":403})));
        assert!(changes.take().unwrap().is_err());
    }
}
