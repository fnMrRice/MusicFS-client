use super::*;
use egui::{
    accesskit::{Action, ActionRequest, NodeId},
    Event,
};
use musicfs_player_core::Player;
use musicfs_server_adapter::ServerSession;
use serde_json::json;
use std::{
    sync::mpsc::{channel, Receiver},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

async fn fixture() -> (ServerSession, Receiver<(String, Option<String>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (requests, received) = channel();
    tokio::spawn(async move {
        loop {
            let (connection, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(connection);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let Some(path) = line.split_whitespace().nth(1) else {
                continue;
            };
            let url = url::Url::parse(&format!("http://localhost{path}")).unwrap();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            let body = match url.path() {
                "/api/v1/health/ready" => {
                    json!({"ready":true,"product":"MusicFS","apiVersion":"1","instanceId":"test","serverVersion":"1"})
                }
                "/api/v1/auth/session" => {
                    json!({"user":{"id":"test","role":"user"},"csrfToken":"test"})
                }
                "/api/v1/library/tracks" | "/api/v1/library/albums" => {
                    let list = url.path().rsplit('/').next().unwrap();
                    let after = url
                        .query_pairs()
                        .find(|(k, _)| k == "after")
                        .map(|(_, v)| v.into_owned());
                    let first = after.is_none();
                    requests.send((list.to_owned(), after)).unwrap();
                    let rows = if first { 0..50 } else { 50..51 };
                    let items: Vec<_> = rows
                        .map(|i| {
                            let id = format!("{list}-{i}");
                            if list == "tracks" {
                                json!({"trackId":id,"title":id,"artists":[],"albums":[]})
                            } else {
                                json!({"albumId":id,"name":id,"trackCount":1})
                            }
                        })
                        .collect();
                    json!({"items":items,"nextCursor":first.then_some("page-two")})
                }
                _ => json!({"items":[]}),
            };
            let body = body.to_string();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: musicfs_session=test; Path=/api/v1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            reader
                .get_mut()
                .write_all(response.as_bytes())
                .await
                .unwrap();
        }
    });
    (
        ServerSession::login(&endpoint, "test").await.unwrap(),
        received,
    )
}

fn frame(ui: &mut SignedUi, ctx: &egui::Context, svc: Services<'_>, events: Vec<Event>) -> NodeId {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(960., 520.),
        )),
        events,
        ..Default::default()
    };
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |panel| ui.page(panel, svc));
    });
    output
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find(|(_, n)| n.label() == Some(data::CategoryTab::Album.label()))
        .expect("Missing album tab")
        .0
}

fn wait_for_rows(ui: &mut SignedUi, ctx: &egui::Context, svc: Services<'_>, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        frame(ui, ctx, svc, vec![]);
        let (count, error) = if ui.catalog.tabs == data::CategoryTab::Album {
            (
                ui.catalog.categories.items.len(),
                &ui.catalog.categories.error,
            )
        } else {
            (ui.catalog.tracks.items.len(), &ui.catalog.tracks.error)
        };
        assert!(error.is_none(), "Page request failed: {error:?}");
        if count == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Expected {expected} rows, got {count}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn tracks_and_albums_load_on_entry_and_continue_when_scrolled_to_the_end() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (server, requests) = runtime.block_on(fixture());
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
    let mut ui = SignedUi::default();
    for list in ["tracks", "albums"] {
        if list == "albums" {
            let album = frame(&mut ui, &ctx, svc, vec![]);
            frame(
                &mut ui,
                &ctx,
                svc,
                vec![
                    Event::PointerGone,
                    Event::AccessKitActionRequest(ActionRequest {
                        action: Action::Click,
                        target: album,
                        data: None,
                    }),
                ],
            );
            assert!(ui.catalog.tabs == data::CategoryTab::Album);
        }
        wait_for_rows(&mut ui, &ctx, svc, 50);
        // 页首停留多个绘制周期，不能提前请求第二页。
        for _ in 0..3 {
            frame(&mut ui, &ctx, svc, vec![]);
            assert!(!ui.catalog.tracks.busy() && !ui.catalog.categories.busy());
        }
        assert_eq!(
            requests.try_iter().collect::<Vec<_>>(),
            [(list.to_owned(), None)]
        );
        frame(
            &mut ui,
            &ctx,
            svc,
            vec![
                Event::PointerMoved(egui::pos2(450., 420.)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Page,
                    delta: egui::vec2(0., -15.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        wait_for_rows(&mut ui, &ctx, svc, 51);
        assert_eq!(
            requests.try_iter().collect::<Vec<_>>(),
            [(list.to_owned(), Some("page-two".into()))]
        );
    }
}
