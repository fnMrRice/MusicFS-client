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

#[test]
fn expired_pages_survive_failed_revalidation_then_replace_items_and_cursors_together() {
    let ctx = Context::default();
    let runtime = Builder::new_current_thread().build().unwrap();
    let mut pages = Pages::default();
    let run = |pages: &mut Pages<u8>| {
        runtime.block_on(tokio::task::yield_now());
        pages.poll();
    };
    let page = |item, cursor: &str| {
        Ok(Page {
            items: vec![item],
            next_cursor: Some(cursor.to_owned()),
        })
    };
    for response in [page(1, "two"), page(2, "three")] {
        pages.load(&ctx, runtime.handle(), std::future::ready(response));
        run(&mut pages);
    }
    assert_eq!(pages.request_cursor().as_deref(), Some("three"));

    // 上一页离开时已请求续页。过期后应从首页重验，旧续页不能追加到新快照。
    let (deliver_old_page, old_page) = tokio::sync::oneshot::channel();
    pages.load(&ctx, runtime.handle(), async { old_page.await.unwrap() });
    assert!(pages.busy());
    pages.resume(Duration::ZERO);
    assert!(!pages.busy());
    assert!(!pages.started);
    assert_eq!(pages.request_cursor(), None);
    let _ = deliver_old_page.send(page(99, "old-cursor"));

    // 失败时仍须展示完整的旧分页，并能直接重试首页。
    pages.load(&ctx, runtime.handle(), async { Err("Offline".into()) });
    assert_eq!(pages.items, [1, 2]);
    run(&mut pages);
    assert_eq!(pages.error.as_deref(), Some("Offline"));
    assert_eq!(pages.items, [1, 2]);
    assert_eq!(pages.next.as_deref(), Some("three"));
    assert_eq!(pages.request_cursor(), None);

    // 新快照可以再次使用旧游标，后续请求须使用新快照中的游标。
    pages.load(&ctx, runtime.handle(), std::future::ready(page(3, "two")));
    assert_eq!(pages.items, [1, 2]);
    run(&mut pages);
    assert!(pages.error.is_none());
    assert_eq!(pages.items, [3]);
    assert_eq!(pages.request_cursor().as_deref(), Some("two"));
    pages.load(&ctx, runtime.handle(), std::future::ready(page(4, "three")));
    run(&mut pages);
    assert!(pages.can_load_more());
    assert_eq!(pages.items, [3, 4]);
}

#[test]
fn a_trimmed_cache_resumes_after_the_last_retained_row() {
    let ctx = Context::default();
    let runtime = Builder::new_current_thread().build().unwrap();
    let mut pages = Pages::default();
    let append = |pages: &mut Pages<u8>, item: u8, next: &str| {
        let page = Page {
            items: vec![item],
            next_cursor: Some(next.to_owned()),
        };
        pages.load(&ctx, runtime.handle(), async { Ok(page) });
        runtime.block_on(tokio::task::yield_now());
        pages.poll();
    };
    append(&mut pages, 1, "two");
    append(&mut pages, 2, "three");
    append(&mut pages, 3, "four");
    pages.limit_cached_rows(2);
    assert_eq!(pages.items, [1]);
    assert_eq!(pages.request_cursor().as_deref(), Some("two"));
    append(&mut pages, 2, "three");
    assert!(pages.error.is_none());
    assert_eq!(pages.items, [1, 2]);
}
