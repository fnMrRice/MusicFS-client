use super::*;
use tokio::runtime::Builder;

#[test]
fn pagination_stops_while_busy_after_failure_and_at_the_end() {
    let ctx = Context::default();
    let runtime = Builder::new_current_thread().build().unwrap();
    let mut pages = Pages::default();
    let page = |items, cursor: Option<&str>| Page {
        items,
        next_cursor: cursor.map(str::to_owned),
    };
    let deliver = |pages: &mut Pages<u8>, result| {
        pages.load(&ctx, runtime.handle(), std::future::ready(result));
        assert!(!pages.can_load_more());
        runtime.block_on(tokio::task::yield_now());
        pages.poll();
    };

    // 空页也可能带有下一页。失败时等待手动重试，并保留该页的游标。
    deliver(&mut pages, Ok(page(vec![], Some("a"))));
    assert!(pages.can_load_more());
    deliver(&mut pages, Ok(page(vec![1], Some("b"))));
    assert!(pages.can_load_more());

    for result in [
        Err("Connection interrupted".into()),
        Ok(page(vec![2], Some("a"))),
        Ok(page(vec![2], Some(""))),
    ] {
        deliver(&mut pages, result);
        assert!(!pages.can_load_more());
        assert!(pages.error.is_some());
        assert_eq!(pages.items, [1]);
        assert_eq!(pages.next.as_deref(), Some("b"));
    }

    deliver(&mut pages, Ok(page(vec![2], None)));
    assert!(pages.error.is_none());
    assert!(!pages.can_load_more());
    assert_eq!(pages.items, [1, 2]);
}
