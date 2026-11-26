use super::*;
use crate::ui::{data::CategoryTab, http_fixture::session_with_response};
use egui::accesskit::{Action, ActionRequest, NodeId};
use image::ImageEncoder;
use musicfs_player_core::Player;
use musicfs_server_adapter::CategoryKind;
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn cover_png(gray: u8) -> Vec<u8> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[gray; 6 * 6 * 3], 6, 6, image::ExtendedColorType::Rgb8)
        .unwrap();
    png
}

#[test]
fn album_gallery_uses_local_artwork_and_only_refetches_after_manual_update() {
    const PATH: &str = "/api/v1/library/albums/record/artwork";
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    let server = runtime.block_on(session_with_response(move |method, url| {
        let mut log = recorded.lock().unwrap();
        let updated = log.iter().any(|(method, _)| method == "POST");
        log.push((method.to_owned(), url.path().to_owned()));
        match (method, url.path()) {
            ("GET", "/api/v1/library/albums") => {
                let page = json!({"items":[
                    {"albumId":"record","name":"The Album","trackCount":3,"coverPath":PATH},
                    {"albumId":"external","name":"Other","trackCount":1,"coverPath":"https://example.org/untrusted.png"}
                ],"nextAfter":null});
                ("200 OK","application/json",page.to_string().into_bytes())
            }
            ("GET", PATH) => {
                ("200 OK","image/png",cover_png(if updated {183} else {43}))
            }
            ("POST", "/api/v1/library/albums/record/artwork/refresh") => ("204 No Content","text/plain",vec![]),
            _ => ("404 Not Found","text/plain",vec![])
        }
    }));
    let page = runtime
        .block_on(server.categories(CategoryKind::Album, None, "", None))
        .unwrap();
    assert_eq!(page.items[0].cover_path.as_deref(), Some(PATH));
    assert!(page.items[1].cover_path.is_none());
    let activity = server.listen_activity(|| {});
    let player = Player::new(|_| Err("Unexpected playback".into())).unwrap();
    let svc = Services {
        runtime: runtime.handle(),
        server: &server,
        activity: &activity,
        player: &player,
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut cache = Artwork::default();
    let mut detail = AlbumCover::default();
    let mut frame = |gallery: bool, click: Option<NodeId>| {
        let events = click
            .map(|target| {
                egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target,
                    data: None,
                })
            })
            .into_iter()
            .collect();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1120., 560.),
            )),
            events,
            ..Default::default()
        };
        ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                detail.poll(&mut cache);
                cache.poll(ctx);
                if gallery {
                    super::super::Catalog::category_collection(
                        ui,
                        svc,
                        &page.items,
                        CategoryTab::Album,
                        false,
                        &mut cache,
                    );
                } else {
                    detail.show(ui, svc, &mut cache, "record", Some(&page.items[0]));
                }
            });
        })
    };
    for gray in [43, 183] {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let output = frame(gray == 43, None);
            if output.textures_delta.set.iter().any(|(_, delta)| matches!(&delta.image,
                egui::ImageData::Color(image) if image.pixels.first()==Some(&egui::Color32::from_gray(gray)))) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "No album texture with expected pixels ({gray})"
            );
            std::thread::sleep(Duration::from_millis(8));
        }
        if gray == 43 {
            frame(false, None);
            let tree = frame(false, None).platform_output.accesskit_update.unwrap();
            let (target, _) = tree
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some(tr("更新封面")))
                .unwrap();
            frame(false, Some(*target));
        }
    }
    for _ in 0..8 {
        frame(true, None);
    }
    let seen = requests.lock().unwrap();
    let image_calls: Vec<_> = seen
        .iter()
        .filter(|(_, path)| path.contains("/artwork"))
        .map(|(method, path)| (method.as_str(), path.as_str()))
        .collect();
    assert_eq!(
        image_calls,
        [
            ("GET", PATH),
            ("POST", "/api/v1/library/albums/record/artwork/refresh"),
            ("GET", PATH)
        ]
    );
}

#[test]
fn reopening_an_album_credit_displays_the_new_local_cover() {
    const ALBUM: &str = "/api/v1/library/albums/record";
    const REMOTE: &str = "/api/v1/library/albums/record/artwork";
    const MEDIA: &str = "/api/v1/media/local";
    let local = Arc::new(AtomicBool::new(false));
    let selected = Arc::clone(&local);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(session_with_response(move |_, url| match url.path() {
        ALBUM => {
            let path = if selected.load(Ordering::Relaxed) {
                MEDIA
            } else {
                REMOTE
            };
            (
                "200 OK",
                "application/json",
                json!({"coverPath":path}).to_string().into_bytes(),
            )
        }
        REMOTE => ("200 OK", "image/png", cover_png(43)),
        MEDIA => ("200 OK", "image/png", cover_png(183)),
        "/api/v1/provider-instances" => ("200 OK", "application/json", br#"{"items":[]}"#.to_vec()),
        _ => ("404 Not Found", "text/plain", vec![]),
    }));
    let player = Player::new(|_| Err("Unexpected playback".into())).unwrap();
    let activity = server.listen_activity(|| {});
    let svc = Services {
        runtime: runtime.handle(),
        server: &server,
        activity: &activity,
        player: &player,
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut catalog = super::super::Catalog::default();
    for (manual, gray) in [(false, 43), (true, 183)] {
        local.store(manual, Ordering::Relaxed);
        catalog.open_credit(CategoryTab::Album, "record".into(), "The Album".into());
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let out = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    catalog.library(ui, svc);
                });
            });
            if out.textures_delta.set.iter().any(|(_, delta)| matches!(&delta.image,
                egui::ImageData::Color(img) if img.pixels.first()==Some(&egui::Color32::from_gray(gray)))) {
                let tree = out.platform_output.accesskit_update.unwrap();
                let can_update = tree.nodes.iter().any(|(_, node)| node.label()==Some(tr("更新封面")));
                assert_eq!(can_update, !manual);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Album credit did not show the current cover"
            );
            std::thread::sleep(Duration::from_millis(8));
        }
    }
}

#[test]
fn refreshing_albums_retries_failed_thumbnails_from_the_server_cache() {
    use std::sync::atomic::AtomicUsize;

    const COVER: &str = "/api/v1/library/albums/slow/artwork";
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(session_with_response(move |method, url| match url.path() {
        "/api/v1/library/albums" => (
            "200 OK",
            "application/json",
            json!({"items":[
                {"albumId":"slow","name":"Slow album","trackCount":2,"coverPath":COVER}
            ]})
            .to_string()
            .into_bytes(),
        ),
        COVER => {
            assert_eq!(method, "GET");
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                (
                    "503 Service Unavailable",
                    "application/json",
                    b"{}".to_vec(),
                )
            } else {
                ("200 OK", "image/png", cover_png(91))
            }
        }
        "/api/v1/provider-instances" => ("200 OK", "application/json", br#"{"items":[]}"#.to_vec()),
        _ => ("404 Not Found", "text/plain", vec![]),
    }));
    let activity = server.listen_activity(|| {});
    let player = Player::new(|_| Err("Unexpected playback".into())).unwrap();
    let svc = Services {
        runtime: runtime.handle(),
        server: &server,
        activity: &activity,
        player: &player,
    };
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut catalog = super::super::Catalog {
        tabs: CategoryTab::Album,
        ..Default::default()
    };
    let mut frame = |events| {
        ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| catalog.library(ui, svc));
            },
        )
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while attempts.load(Ordering::SeqCst) == 0 {
        frame(vec![]);
        assert!(Instant::now() < deadline, "Missing first thumbnail request");
        std::thread::sleep(Duration::from_millis(8));
    }
    let tree = frame(vec![]).platform_output.accesskit_update.unwrap();
    let (target, _) = tree
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(tr("刷新分类")))
        .unwrap();
    frame(vec![egui::Event::AccessKitActionRequest(ActionRequest {
        target: *target,
        action: Action::Click,
        data: None,
    })]);
    loop {
        let output = frame(vec![]);
        if output.textures_delta.set.iter().any(|(_, delta)| matches!(&delta.image,
            egui::ImageData::Color(img) if img.pixels.first() == Some(&egui::Color32::from_gray(91)))) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Missing recovered album thumbnail"
        );
        std::thread::sleep(Duration::from_millis(8));
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}
