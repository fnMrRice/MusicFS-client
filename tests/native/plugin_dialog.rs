use super::{DialogAction, Editor, Submission};
use crate::i18n::tr;
use eframe::egui::{
    self,
    accesskit::{Action, ActionRequest, Node, NodeId, Role, TreeUpdate},
};
use serde_json::json;

fn editor() -> Editor {
    let mut fields = vec![json!({
        "key": "enabled", "type": "checkbox", "label": "Enable proxy", "defaultValue": false
    })];
    fields.push(json!({
        "key": "mode", "type": "select", "label": "Connection mode", "defaultValue": "auto",
        "options": [{"value": "auto", "label": "A very long network connection option ".repeat(24)}]
    }));
    for i in 1..16 {
        fields.push(json!({
            "key": format!("server-{i}"), "type": "text", "label": format!("Server {i}"),
            "description": "Address of the proxy server.", "defaultValue": ""
        }));
    }
    let mut editor = Editor::new("example", "MusicBrainz");
    editor.apply(
        serde_json::from_value(json!({
            "pluginId": "example", "revision": 7, "values": {},
            "schema": {"title": "Network", "revision": "v1", "actions": [], "fields": fields}
        }))
        .unwrap(),
    );
    editor
}

fn control<'a>(tree: &'a TreeUpdate, label: &str) -> &'a (NodeId, Node) {
    tree.nodes
        .iter()
        .find(|(_, node)| {
            node.label() == Some(label) || node.role() == Role::Label && node.value() == Some(label)
        })
        .unwrap_or_else(|| panic!("Missing control: {label}"))
}

#[test]
fn long_plugin_forms_keep_cancel_and_save_visible_and_submit_edits() {
    for size in [egui::vec2(660., 500.), egui::vec2(1180., 800.)] {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        crate::ui::style::apply(&ctx, "dark", "default");
        ctx.enable_accesskit();
        let mut editor = editor();
        let mut frame = |click: Option<NodeId>| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            if let Some(target) = click {
                input
                    .events
                    .push(egui::Event::AccessKitActionRequest(ActionRequest {
                        action: Action::Click,
                        target,
                        data: None,
                    }));
            }
            let mut action = DialogAction::None;
            let output = ctx.run(input, |ctx| {
                let next = editor.dialog(ctx);
                if !matches!(next, DialogAction::None) {
                    action = next;
                }
            });
            (output.platform_output.accesskit_update.unwrap(), action)
        };
        frame(None);
        let (tree, _) = frame(None);
        let field = |label, role| {
            let label_id = control(&tree, label).0;
            tree.nodes
                .iter()
                .find(|(_, node)| node.role() == role && node.labelled_by().contains(&label_id))
                .unwrap_or_else(|| panic!("Missing field: {label}"))
                .1
                .bounds()
                .unwrap()
        };
        let select = field("Connection mode", Role::ComboBox);
        let input = field("Server 1", Role::TextInput);
        let dialog = ctx
            .memory(|m| m.area_rect(egui::Id::new("plugin-settings-dialog")))
            .unwrap();
        for rect in [input, select] {
            assert!(rect.x0 >= f64::from(dialog.left()) && rect.x1 <= f64::from(dialog.right()));
        }
        assert!(
            (select.x0 - input.x0).abs() <= 2.,
            "{select:?} vs {input:?}"
        );
        assert!(
            (select.x1 - input.x1).abs() <= 2.,
            "{select:?} vs {input:?}"
        );
        let save = &control(&tree, tr("保存设置")).1;
        assert!(save.is_disabled());
        let save = save.bounds().unwrap();
        let cancel = control(&tree, tr("取消")).1.bounds().unwrap();
        for rect in [cancel, save] {
            assert!(rect.x0 >= 0. && rect.x1 <= f64::from(size.x));
            assert!(rect.y0 >= 0. && rect.y1 <= f64::from(size.y));
        }
        assert!((save.y0 + save.y1 - cancel.y0 - cancel.y1).abs() <= 2.);
        assert!(cancel.x1 <= save.x0 || save.x1 <= cancel.x0);

        frame(Some(control(&tree, "Enable proxy").0));
        let (tree, _) = frame(None);
        let (save_id, save) = control(&tree, tr("保存设置"));
        assert!(!save.is_disabled());
        let (_, DialogAction::Submit(Submission::Save(body))) = frame(Some(*save_id)) else {
            panic!("Expected to submit the edited settings");
        };
        assert_eq!(body["values"]["enabled"], true);
        assert_eq!(body["expectedRevision"], 7);
        assert_eq!(body["schemaRevision"], "v1");
        assert!(matches!(
            frame(Some(control(&tree, tr("取消")).0)).1,
            DialogAction::Close
        ));
    }
}
