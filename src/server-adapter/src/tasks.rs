use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPage {
    pub items: Vec<RefreshTask>,
    pub active_count: usize,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTask {
    pub id: String,
    pub provider_name: String,
    pub owner_name: String,
    pub roots: Vec<Root>,
    pub status: String,
    pub cancellable: bool,
}

#[derive(Clone, Deserialize)]
pub struct Root {
    pub name: String,
}

#[derive(Clone, Deserialize)]
pub struct TaskSnapshot {
    pub page: TaskPage,
    pub changed: Vec<RefreshTask>,
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
