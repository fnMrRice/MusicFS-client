use super::*;
use std::future::ready;

fn page(ids: &[&str], next: Option<&str>) -> Page<Track> {
    Page {
        items: ids
            .iter()
            .map(|id| Track {
                id: (*id).into(),
                title: String::new(),
                artists: vec![],
                album: None,
                source_track_id: None,
                forced_source_track_id: None,
                duration_ms: None,
                cover_object_id: None,
            })
            .collect(),
        next_cursor: next.map(str::to_owned),
    }
}

#[test]
fn loads_existing_favorites_from_every_page_and_applies_concurrent_changes() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut favorites = Favorites::default();
    assert_eq!(favorites.contains("first", false), None);
    assert_eq!(favorites.contains("first", true), Some(true));
    let ids = rt
        .block_on(collect_ids(|cursor| {
            let value = match cursor.as_deref() {
                None => page(&["first", "removed"], Some("page2")),
                Some("page2") => page(&["later"], None),
                _ => panic!("Unexpected cursor"),
            };
            ready(Ok(value))
        }))
        .unwrap();
    favorites.record("added".into(), true);
    favorites.record("removed".into(), false);
    assert_eq!(favorites.contains("removed", true), Some(false));
    favorites.install(ids);
    for (id, expected) in [
        ("first", true),
        ("later", true),
        ("added", true),
        ("removed", false),
        ("other", false),
    ] {
        assert_eq!(favorites.contains(id, false), Some(expected));
    }
    favorites.record("first".into(), false);
    favorites.record("other".into(), true);
    assert_eq!(favorites.contains("first", false), Some(false));
    assert_eq!(favorites.contains("other", false), Some(true));
}

#[test]
fn failed_or_repeating_pages_do_not_install_partial_favorites() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let failed = rt.block_on(collect_ids(|cursor| {
        ready(match cursor {
            None => Ok(page(&["partial"], Some("next"))),
            Some(_) => Err("Unavailable".into()),
        })
    }));
    assert_eq!(failed.unwrap_err(), "Unavailable");
    let cycle = rt.block_on(collect_ids(|_| ready(Ok(page(&[], Some("repeated"))))));
    assert!(cycle.is_err());
}
