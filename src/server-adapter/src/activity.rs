use super::{
    control::ControlSocket,
    tasks::{Subscription, TaskUpdates},
    ServerSession, API_TIMEOUT,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::{sync::watch, task::JoinHandle, time};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ActivityRevisions {
    pub capabilities: u64,
    pub scans: u64,
    pub plugin_jobs: u64,
}

pub struct ActivityStream {
    revisions: watch::Receiver<ActivityRevisions>,
    tasks: watch::Sender<Option<Subscription>>,
    worker: JoinHandle<()>,
}

impl ActivityStream {
    pub fn revisions(&self) -> ActivityRevisions {
        *self.revisions.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<ActivityRevisions> {
        self.revisions.clone()
    }

    pub fn subscribe_tasks(&self, all: bool) -> TaskUpdates {
        let (subscription, updates) = Subscription::new(all);
        self.tasks.send_replace(Some(subscription));
        updates
    }
}

impl Drop for ActivityStream {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

impl ServerSession {
    /// 每个已登录界面共享一条通知连接。重连后重新读取当前可见状态。
    pub fn listen_activity(&self, repaint: impl Fn() + Send + Sync + 'static) -> ActivityStream {
        let (tx, revisions) = watch::channel(ActivityRevisions::default());
        let (tasks, mut requested) = watch::channel(None);
        let session = self.clone();
        let worker = self.runtime.spawn(async move {
            let mut backoff = Duration::from_secs(1);
            while session.ensure_active().is_ok() && !session.expired() {
                if let Ok(Ok((socket, ready))) =
                    time::timeout(API_TIMEOUT, session.control_socket()).await
                {
                    if let Some((interval, timeout)) = heartbeat(&ready) {
                        backoff = Duration::from_secs(1);
                        tx.send_modify(|r| {
                            r.capabilities += 1;
                            r.scans += 1;
                            r.plugin_jobs += 1;
                        });
                        repaint();
                        listen(socket, interval, timeout, &tx, &mut requested, &repaint).await;
                    }
                }
                if session.expired() {
                    repaint();
                    break;
                }
                time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        });
        ActivityStream {
            revisions,
            tasks,
            worker,
        }
    }
}

fn heartbeat(ready: &Value) -> Option<(Duration, Duration)> {
    let interval = ready["heartbeatIntervalMs"].as_u64()?;
    let timeout = ready["heartbeatTimeoutMs"].as_u64()?;
    if interval == 0 || timeout <= interval || timeout > 300_000 {
        return None;
    }
    Some((
        Duration::from_millis(interval),
        Duration::from_millis(timeout),
    ))
}

async fn listen(
    mut socket: ControlSocket,
    interval: Duration,
    timeout: Duration,
    tx: &watch::Sender<ActivityRevisions>,
    requested: &mut watch::Receiver<Option<Subscription>>,
    repaint: &impl Fn(),
) {
    let mut tick = time::interval_at(time::Instant::now() + interval, interval);
    tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut last_pong = time::Instant::now();
    let mut task_subscription: Option<(String, Subscription)> = None;
    // 新连接的首个快照必须来自本次订阅，即使任务范围与断线前相同。
    requested.mark_changed();
    loop {
        tokio::select! {
            request = requested.changed() => {
                if request.is_err() { return; }
                let next = requested.borrow_and_update().clone();
                if let Some(subscription) = next {
                    let (id, frame) = subscription.request();
                    if !matches!(time::timeout(interval, socket.send(frame)).await, Ok(Ok(()))) { return; }
                    task_subscription = Some((id, subscription));
                }
            }
            _ = tick.tick() => {
                if last_pong.elapsed() >= timeout { return; }
                let ping = socket.send(Message::text(r#"{"type":"ping"}"#));
                if !matches!(time::timeout(interval, ping).await, Ok(Ok(()))) { return; }
            }
            message = socket.next() => {
                let Some(Ok(message)) = message else { return; };
                match message {
                    Message::Close(_) => return,
                    Message::Text(text) => {
                        let Ok(frame) = serde_json::from_str::<Value>(&text) else { return; };
                        if frame["type"] == "pong" { last_pong = time::Instant::now(); }
                        let tasks_changed = task_subscription.as_ref()
                            .is_some_and(|(id, subscription)| subscription.receive(id, &frame));
                        if tx.send_if_modified(|r| update(r, &frame)) || tasks_changed { repaint(); }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn update(revisions: &mut ActivityRevisions, frame: &Value) -> bool {
    if frame["type"] != "event" {
        return false;
    }
    match frame["event"].as_str() {
        Some("capabilities.changed") => {
            revisions.capabilities += 1;
            true
        }
        Some("activity.changed") => {
            let Some(resources) = frame["data"]["resources"].as_array() else {
                return false;
            };
            let scan = resources.iter().any(|r| r == "scans");
            let plugin = resources.iter().any(|r| r == "pluginJobs");
            revisions.scans += u64::from(scan);
            revisions.plugin_jobs += u64::from(plugin);
            scan || plugin
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionInfo;
    use reqwest::cookie::Jar;
    use serde_json::json;
    use std::sync::{atomic::AtomicBool, Arc};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    #[tokio::test]
    async fn reconnect_authenticates_again_and_invalidates_open_views() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let origin = url::Url::parse(&endpoint).unwrap();
        let cookies = Arc::new(Jar::default());
        cookies.add_cookie_str("musicfs_session=fixture; Path=/api/v1", &origin);
        let session = ServerSession {
            info: SessionInfo {
                endpoint,
                instance_id: "fixture".into(),
                user_name: "listener".into(),
                server_version: "1".into(),
                role: "user".into(),
            },
            origin,
            client: reqwest::Client::new(),
            image_client: crate::covers::image_client().unwrap(),
            cookies,
            csrf: "test-csrf".into(),
            active: Arc::new(AtomicBool::new(true)),
            expired: Arc::default(),
            runtime: tokio::runtime::Handle::current(),
        };

        let peer = tokio::spawn(async move {
            let mut previous_id = String::new();
            for connection in 0..2 {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut ws =
                    tokio_tungstenite::accept_hdr_async(tcp, |req: &Request, mut res: Response| {
                        assert_eq!(req.uri().path(), "/api/v1/control");
                        assert_eq!(req.headers()["cookie"], "musicfs_session=fixture");
                        res.headers_mut().insert(
                            "sec-websocket-protocol",
                            "musicfs.control.v1".parse().unwrap(),
                        );
                        Ok(res)
                    })
                    .await
                    .unwrap();
                let auth = ws.next().await.unwrap().unwrap();
                assert_eq!(
                    serde_json::from_str::<Value>(auth.to_text().unwrap()).unwrap(),
                    json!({"type":"auth", "csrfToken":"test-csrf"})
                );
                ws.send(Message::text(
                    json!({"type":"ready", "instanceId":"fixture",
                    "heartbeatIntervalMs":50, "heartbeatTimeoutMs":1000})
                    .to_string(),
                ))
                .await
                .unwrap();
                let request = ws.next().await.unwrap().unwrap();
                let request: Value = serde_json::from_str(request.to_text().unwrap()).unwrap();
                assert_eq!(request["type"], "request");
                assert_eq!(request["method"], "POST");
                assert_eq!(request["path"], "/tasks/subscription");
                assert_eq!(request["body"]["scope"], "mine");
                let id = request["body"]["subscriptionId"].as_str().unwrap();
                assert_ne!(id, previous_id);
                previous_id = id.into();
                ws.send(Message::text(
                    json!({"type":"response","id":request["id"],"status":204}).to_string(),
                ))
                .await
                .unwrap();
                ws.send(Message::text(json!({"type":"event","event":"tasks.snapshot","data":{
                    "subscriptionId":id,"page":{"items":[],"activeCount":connection+1,"nextCursor":null},
                    "changed":[],"reset":true}}).to_string())).await.unwrap();
                if connection == 0 {
                    ws.send(Message::text(
                        json!({"type":"event","event":"activity.changed",
                        "data":{"resources":["scans","scans"]}})
                        .to_string(),
                    ))
                    .await
                    .unwrap();
                    let ping = ws.next().await.unwrap().unwrap();
                    assert_eq!(
                        serde_json::from_str::<Value>(ping.to_text().unwrap()).unwrap(),
                        json!({"type":"ping"})
                    );
                    ws.close(None).await.unwrap();
                } else {
                    // 销毁订阅后应立即断开，服务端不必等待鉴权会话过期。
                    while let Some(Ok(frame)) = ws.next().await {
                        if frame.is_close() {
                            break;
                        }
                    }
                }
            }
        });
        let (wake, mut repaints) = tokio::sync::mpsc::unbounded_channel();
        let stream = session.listen_activity(move || {
            let _ = wake.send(());
        });
        let mut tasks = stream.subscribe_tasks(false);
        let mut changes = stream.subscribe();
        time::timeout(
            Duration::from_secs(6),
            changes.wait_for(|r| r.scans == 3 && r.plugin_jobs == 2),
        )
        .await
        .unwrap()
        .unwrap();
        time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(result) = tasks.take() {
                    if result.unwrap().page.active_count == 2 {
                        break;
                    }
                }
                repaints.recv().await.unwrap();
            }
        })
        .await
        .unwrap();
        drop(stream);
        time::timeout(Duration::from_secs(2), peer)
            .await
            .unwrap()
            .unwrap();
    }
}
