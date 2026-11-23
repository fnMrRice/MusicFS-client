use super::*;
use musicfs_player_core::Player;
use schema::Settings;
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn builds_and_validates_plugin_owned_fields() {
    let mut fields = vec![];
    for (key, kind, default) in [
        ("proxy", "text", json!("")),
        ("quality", "number", json!(4)),
        ("mode", "select", json!("direct")),
        ("enabled", "checkbox", json!(false)),
    ] {
        fields.push(json!({"key": key, "label": key, "type": kind, "defaultValue": default}));
    }
    fields[0]["required"] = json!(true);
    fields[0]["maxLength"] = json!(4);
    fields[1]["min"] = json!(2);
    fields[1]["max"] = json!(10);
    fields[1]["step"] = json!(2);
    fields[2]["options"] = json!([{"value":"direct","label":"Direct"}]);
    let data: Settings = serde_json::from_value(json!({
        "pluginId":"example", "revision":7, "values":{"proxy":"适配"},
        "schema":{"title":"Test","revision":"v1","actions":[],"fields":fields}
    }))
    .unwrap();
    let draft = data.draft();
    assert_eq!(data.schema.values(&draft).unwrap()["quality"], json!(4));
    for (key, raw) in [
        ("proxy", "   "),
        ("proxy", "😁😁😁"),
        ("quality", "7"),
        ("quality", "12"),
        ("quality", "NaN"),
        ("mode", "unknown"),
        ("enabled", "yes"),
    ] {
        let mut invalid = draft.clone();
        invalid.insert(key.into(), raw.into());
        assert!(data.schema.values(&invalid).is_none(), "{key}");
    }
}

#[test]
fn rejects_path_separators_and_parent_identifiers() {
    for id in [".", "..", "a/b", "x?other=true", "%2e%2e"] {
        assert!(segment(id).is_err());
    }
    assert_eq!(
        segment("musicfs.scraper.local"),
        Ok("musicfs.scraper.local")
    );
}

// 只在临时服务端运行，会保存一次实例开关并恢复原值。
#[test]
#[ignore]
fn browses_plugins_and_changes_an_isolated_instance() {
    let endpoint = std::env::var("MUSICFS_TEST_ENDPOINT").unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt
        .block_on(ServerSession::login(
            &endpoint,
            &std::env::var("MUSICFS_TEST_TOKEN").unwrap(),
        ))
        .unwrap();
    let player = Player::new(|_| Err("No playback requested".to_owned())).unwrap();
    let svc = Services {
        server: &server,
        runtime: rt.handle(),
        player: &player,
    };
    let ctx = egui::Context::default();
    let mut plugins = Plugins::default();
    let tick = |plugins: &mut Plugins| {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| plugins.show(ui, svc));
        });
        assert!(plugins.list.error.is_none(), "{:?}", plugins.list.error);
        assert!(plugins.online.error.is_none(), "{:?}", plugins.online.error);
    };
    let wait = |plugins: &mut Plugins, done: &dyn Fn(&Plugins) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            tick(plugins);
            if done(plugins) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Plugin changes did not become visible"
            );
            std::thread::sleep(Duration::from_millis(15));
        }
    };
    wait(&mut plugins, &|p| {
        p.list.data.is_some() && p.online.data.is_some()
    });
    let snapshot = plugins.list.data.as_ref().unwrap();
    assert!(snapshot
        .items
        .iter()
        .any(|p| p.storage.is_some() && p.required));
    let local = snapshot
        .items
        .iter()
        .find(|p| p.plugin_id == "musicfs.plugin.source.localfiles")
        .unwrap();
    assert!(local.matches(&BTreeSet::from(["music-source".into()]), "localfiles"));
    assert!(!local.matches(&BTreeSet::from(["database".into()]), ""));
    let id = local.plugin_id.clone();
    let instance = local.instances[0].instance_id.clone();
    let original = local.instances[0].configured_enabled;
    plugins.expanded.insert(id.clone());
    for enabled in [!original, original] {
        let revision = plugins.list.data.as_ref().unwrap().revision.clone();
        plugins
            .switches
            .start(&ctx, svc, &id, Some(&instance), enabled, &revision);
        wait(&mut plugins, &|p| {
            !p.switches.busy()
                && p.list.data.as_ref().is_some_and(|s| {
                    s.items
                        .iter()
                        .find(|p| p.plugin_id == id)
                        .unwrap()
                        .instances[0]
                        .configured_enabled
                        == enabled
                })
        });
    }
    rt.block_on(server.logout()).unwrap();
}
