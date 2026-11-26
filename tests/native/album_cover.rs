use super::*;
use crate::ui::{data::CategoryTab, http_fixture::session_with_response};
use egui::accesskit::{Action, ActionRequest, NodeId};
use image::ImageEncoder;
use musicfs_player_core::Player;
use musicfs_server_adapter::CategoryKind;
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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
                let gray = if updated {183} else {43};
                let mut png = Vec::new();
                image::codecs::png::PngEncoder::new(&mut png).write_image(
                    &[gray; 6 * 6 * 3], 6, 6, image::ExtendedColorType::Rgb8
                ).unwrap();
                ("200 OK","image/png",png)
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
