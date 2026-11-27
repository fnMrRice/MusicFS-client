use super::*;
use egui::{
    accesskit::{Node, Role, TreeUpdate},
    Event, FullOutput, Modifiers, PointerButton, Pos2, Rect,
};

fn frame(
    ctx: &egui::Context,
    login: &mut Login,
    state: &mut DesktopState,
    events: Vec<Event>,
) -> FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1232., 800.))),
            events,
            ..Default::default()
        },
        |ctx| login.show(ctx, state),
    )
}

fn field<'a>(tree: &'a TreeUpdate, role: Role, label: &str) -> &'a Node {
    let label_id = tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Label && node.value() == Some(tr(label)))
        .unwrap_or_else(|| panic!("Missing label: {label}"))
        .0;
    &tree
        .nodes
        .iter()
        .find(|(_, node)| node.role() == role && node.labelled_by().contains(&label_id))
        .unwrap_or_else(|| panic!("Missing field: {label}"))
        .1
}

#[test]
fn login_fields_accept_clicks_in_the_padding_and_keep_saved_login_actions_visible() {
    let ctx = egui::Context::default();
    crate::fonts::install(&ctx);
    style::apply(&ctx, "dark", "miku");
    ctx.enable_accesskit();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut state = DesktopState::at_settings(runtime.handle().clone(), None);
    state.prefs.saved_login = Some("fixture".into());
    state.error = Some("The saved MusicFS server identity has changed. Sign in manually.".into());
    let mut login = Login {
        endpoint: state.prefs.endpoint.clone(),
        ..Default::default()
    };
    frame(&ctx, &mut login, &mut state, vec![]);
    let output = frame(&ctx, &mut login, &mut state, vec![]);
    let tree = output.platform_output.accesskit_update.unwrap();
    let endpoint = field(&tree, Role::TextInput, "服务器地址")
        .bounds()
        .unwrap();
    let token = field(&tree, Role::PasswordInput, "管理员登录令牌")
        .bounds()
        .unwrap();
    assert!(endpoint.height() >= 40. && token.height() >= 40.);
    assert!(endpoint.y1 < token.y0);
    assert!((endpoint.x0 - token.x0).abs() <= 1. && (endpoint.x1 - token.x1).abs() <= 1.);
    for label in ["重试已保存的登录", "忘记已保存的登录", "语言"] {
        assert!(
            output.shapes.iter().any(|clipped| {
                matches!(&clipped.shape, egui::Shape::Text(text)
                if text.galley.text() == tr(label)
                    && clipped.clip_rect.contains_rect(text.visual_bounding_rect()))
            }),
            "Clipped or missing control: {label}"
        );
    }

    let pos = egui::pos2(token.x1 as f32 - 4., token.y0 as f32 + 5.);
    let click = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        &ctx,
        &mut login,
        &mut state,
        vec![Event::PointerMoved(pos), click(true)],
    );
    frame(&ctx, &mut login, &mut state, vec![click(false)]);
    let output = frame(
        &ctx,
        &mut login,
        &mut state,
        vec![Event::Text("test".into())],
    );
    let tree = output.platform_output.accesskit_update.unwrap();
    assert_eq!(login.token, "test");
    assert_eq!(
        field(&tree, Role::PasswordInput, "管理员登录令牌").value(),
        Some("••••")
    );
}
