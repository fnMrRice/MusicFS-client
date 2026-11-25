use super::*;
use tokio::{runtime::Builder, sync::oneshot};

#[test]
fn limits_requests_and_keeps_ready_and_failed_results() {
    let ctx = egui::Context::default();
    let runtime = Builder::new_current_thread().build().unwrap();
    let mut cache = Artwork::default();
    let mut replies = Vec::new();
    for id in ["cover", "missing", "third", "fourth"] {
        let (send, recv) = oneshot::channel();
        replies.push(send);
        cache.touch(id, || {
            Task::spawn(runtime.handle(), &ctx, async { recv.await.unwrap() })
        });
    }
    cache.touch("cover", || panic!("duplicate request"));
    let touched = cache.counter;
    cache.touch("fifth", || panic!("parallel limit exceeded"));
    assert_eq!(cache.pending.len(), MAX_PENDING);

    replies
        .remove(0)
        .send(Ok(ColorImage::new([1, 1], egui::Color32::WHITE)))
        .unwrap();
    replies.remove(0).send(Err("Missing image".into())).unwrap();
    runtime.block_on(tokio::task::yield_now());
    cache.poll(&ctx);
    assert_eq!(cache.pending.len(), 2);
    assert_eq!(cache.items["cover"].0, touched);
    assert!(cache.items["cover"].1.is_some());
    assert!(cache.items["missing"].1.is_none());
    cache.touch("cover", || panic!("ready image requested again"));
    cache.touch("missing", || panic!("failed image requested again"));
    cache.touch("fifth", || {
        Task::spawn(runtime.handle(), &ctx, std::future::pending())
    });
    assert_eq!(cache.pending.len(), 3);
}

#[test]
fn evicts_the_least_recently_used_completed_item_and_preserves_pending_work() {
    let ctx = egui::Context::default();
    let runtime = Builder::new_current_thread().build().unwrap();
    let mut cache = Artwork::default();
    let pending_task = || Task::spawn(runtime.handle(), &ctx, std::future::pending());
    cache.touch("old pending", pending_task);
    for i in 1..MAX_ENTRIES {
        cache.counter += 1;
        cache.items.insert(i.to_string(), (cache.counter, None));
    }
    cache.touch("1", || panic!("cached failure was forgotten"));
    cache.touch("new", pending_task);
    assert!(cache.items.contains_key("1"));
    assert!(!cache.items.contains_key("2"));
    cache.touch("old pending", || panic!("pending request was evicted"));
    cache.touch("2", pending_task);
    assert_eq!(cache.pending.len(), 3);
    assert_eq!(cache.items.len() + cache.pending.len(), MAX_ENTRIES);
}
