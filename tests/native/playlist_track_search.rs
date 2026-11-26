use super::*;
use crate::browse::OnlinePlaylistDetail;
use egui::{
    accesskit::{Action, ActionRequest, Role, TreeUpdate},
    Event, Key, Modifiers,
};
use musicfs_player_core::Player;
use musicfs_server_adapter::ServerSession;
use serde_json::json;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

async fn fixture() -> (ServerSession, Receiver<(String, Option<String>)>) {
    let (requests, received) = channel();
    let server_track = |id: &str| {
        let title = if id == "song-50" {
            "Needle"
        } else {
            "Other title"
        };
        json!({"trackId":id,"title":title,"artists":[{"name":"Performer"}],"albums":[{"name":"Album"}],
            "selectedSource":{"sourceTrackId":id,"providerInstanceId":"files","displayName":title,"availability":"available"}})
    };
    let session = super::http_fixture::session(move |url| match url.path() {
        "/api/v1/playlists/local/snapshots/1" => {
            let offset = url.query_pairs().find(|(k, _)| k == "offset").unwrap().1.into_owned();
            let first = offset == "0";
            requests.send(("local".into(), (!first).then_some(offset))).unwrap();
            let rows = if first { 0..50 } else { 50..51 };
            let mut items: Vec<_> = rows.map(|i| json!({"entryId":format!("e{i}"),"source":{"trackId":format!("song-{i}")}})).collect();
            if !first {
                items.push(json!({"entryId":"extra","source":{"remoteSourceId":"imported"}}));
            }
            json!({"playlistId":"local","revision":1,"nextOffset":first.then_some(50),"items":items})
        }
        path if path.starts_with("/api/v1/library/tracks/song-") => server_track(path.rsplit('/').next().unwrap()),
        "/api/v1/source-refs/lookup" => json!({"items":[{
            "source":{"remoteSourceId":"imported"},
            "remoteTrack":{"externalId":"x","title":"Hidden gem","artists":["Cloud"],"album":"Online"}
        }]}),
        "/api/v1/me/favorites" | "/api/v1/online-instances/test/playlists/remote/tracks" => {
            let online = url.path().ends_with("/tracks");
            let after = url.query_pairs().find(|(k, _)| k == "cursor").map(|(_, v)| v.into_owned());
            let first = after.is_none();
            requests.send((if online { "remote" } else { "favorites" }.into(), after)).unwrap();
            let rows = if first { 0..50 } else { 50..51 };
            let items: Vec<_> = rows.map(|i| {
                let id = format!("song-{i}");
                let title = if i == 50 { "Needle" } else { "Other title" };
                if online {
                    json!({"externalId":id,"position":i,"track":{"externalId":id,"title":title,"artists":["Performer"],"album":"Album"}})
                } else {
                    json!({"musicTrack":server_track(&id)})
                }
            }).collect();
            if online {
                json!({"playlist":{"externalId":"remote","title":"Remote playlist"},"items":items,"nextCursor":first.then_some("page-two"),"complete":true,"observedAt":"2026-01-02"})
            } else {
                json!({"items":items,"nextCursor":first.then_some("page-two")})
            }
        }
        _ => json!({"items":[]}),
    })
    .await;
    (session, received)
}

#[test]
fn all_playlist_routes_search_later_pages_without_scrolling() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (server, requests) = runtime.block_on(fixture());
    let activity = server.listen_activity(|| {});
    let player = Player::new(|_| Err("Unexpected playback".into())).unwrap();
    let svc = Services {
        runtime: runtime.handle(),
        activity: &activity,
        server: &server,
        player: &player,
    };
    let mut signed = SignedUi::default();
    signed.lists.items.push(serde_json::from_value(json!({
        "playlistId":"local", "name":"Local", "snapshotRevision":1,"ruleRevision":1,"stale":false
    })).unwrap());
    for scope in ["favorites", "local", "remote"] {
        let online = scope == "remote";
        signed.set_screen(if scope == "local" {
            Screen::Playlist("local".into())
        } else {
            Screen::Favorites
        });
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut detail = OnlinePlaylistDetail::new("test", "remote", None, false);
        let mut frame = |events: Vec<Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 420.),
                )),
                events,
                ..Default::default()
            };
            ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if online {
                        egui::ScrollArea::vertical()
                            .show(ui, |ui| detail.show(ui, svc, &mut signed.catalog.artwork));
                    } else {
                        signed.page(ui, svc);
                    }
                });
            })
            .platform_output
            .accesskit_update
            .unwrap()
        };
        let tree = wait(&mut frame, |tree| {
            tree.nodes.iter().any(|(_, n)| {
                n.label()
                    .or(n.value())
                    .is_some_and(|s| s.contains("Other title"))
            })
        });
        let (edit, search) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::TextInput)
            .unwrap();
        let sort = tree
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::ComboBox)
            .unwrap()
            .1
            .bounds()
            .unwrap();
        let search = search.bounds().unwrap();
        assert!(search.x1 <= sort.x0 && search.y0 < sort.y1 && sort.y0 < search.y1);
        assert_eq!(
            requests.try_iter().collect::<Vec<_>>(),
            [(scope.into(), None)]
        );
        frame(vec![Event::AccessKitActionRequest(ActionRequest {
            action: Action::Focus,
            target: *edit,
            data: None,
        })]);
        frame(vec![Event::Text("NEEDLE performer album".into())]);
        let tree = wait(&mut frame, |tree| {
            tree.nodes.iter().any(|(_, n)| {
                n.label()
                    .or(n.value())
                    .is_some_and(|s| s.contains("Needle"))
            })
        });
        assert!(!tree.nodes.iter().any(|(_, n)| n
            .label()
            .or(n.value())
            .is_some_and(|s| s.contains("Other title"))));
        assert_eq!(
            requests.try_iter().collect::<Vec<_>>(),
            [(
                scope.into(),
                Some(if scope == "local" { "50" } else { "page-two" }.into())
            )]
        );
        if !online {
            let before = player.snapshot().unwrap().queue.len();
            let enqueue = tree
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some(tr("加入队列")))
                .unwrap()
                .0;
            frame(vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Click,
                target: enqueue,
                data: None,
            })]);
            let queued = player.snapshot().unwrap();
            assert_eq!(queued.queue.len(), before + 1);
            assert_eq!(queued.queue[before].id, "song-50");
            frame(vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Focus,
                target: *edit,
                data: None,
            })]);
        }
        frame(vec![
            Event::Key {
                key: Key::End,
                physical_key: None,
                modifiers: Modifiers::NONE,
                pressed: true,
                repeat: false,
            },
            Event::Text(" nonexistent".into()),
        ]);
        let tree = wait(&mut frame, |tree| {
            tree.nodes
                .iter()
                .any(|(_, n)| n.label().or(n.value()) == Some(tr("没有找到相关曲目")))
        });
        let (clear, _) = tree
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(tr("清除")))
            .unwrap();
        frame(vec![Event::AccessKitActionRequest(ActionRequest {
            action: Action::Click,
            target: *clear,
            data: None,
        })]);
        wait(&mut frame, |tree| {
            tree.nodes.iter().any(|(_, n)| {
                n.label()
                    .or(n.value())
                    .is_some_and(|s| s.contains("Other title"))
            })
        });
        assert!(requests.try_iter().next().is_none());
        if scope == "local" {
            frame(vec![Event::AccessKitActionRequest(ActionRequest {
                action: Action::Focus,
                target: *edit,
                data: None,
            })]);
            frame(vec![Event::Text("GEM online cloud".into())]);
            wait(&mut frame, |tree| {
                tree.nodes.iter().any(|(_, n)| {
                    n.label()
                        .or(n.value())
                        .is_some_and(|s| s.contains("Hidden gem"))
                })
            });
            assert_eq!(signed.catalog.tracks.items.len(), 1);
            assert_eq!(signed.catalog.tracks.items[0].id, "imported");
        } else if !online {
            assert_eq!(signed.catalog.tracks.items.len(), 51);
            assert_eq!(signed.catalog.tracks.items.last().unwrap().id, "song-50");
        }
    }
}

#[track_caller]
fn wait(
    frame: &mut impl FnMut(Vec<Event>) -> TreeUpdate,
    check: impl Fn(&TreeUpdate) -> bool,
) -> TreeUpdate {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let tree = frame(vec![]);
        if check(&tree) {
            return tree;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for playlist content: {:?}",
            tree.nodes
                .iter()
                .filter_map(|(id, n)| n.label().or(n.value()).map(|s| (id, s)))
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(8));
    }
}
