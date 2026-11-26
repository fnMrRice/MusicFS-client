mod avatar;
use super::{
    api::{decode, get, path_id, text, Load},
    Services,
};
use crate::{i18n::tr, ui::style};
use base64::Engine;
use eframe::egui;
use musicfs_server_adapter::ServerSession;
use qrcode::{Color, QrCode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::runtime::Handle;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct View {
    state: String,
    account_id: Option<String>,
    display_name: Option<String>,
    avatar_url: Option<String>,
    membership_tier: Option<String>,
    login_id: Option<String>,
    method: Option<String>,
    qr_url: Option<String>,
    qr_image: Option<String>,
    poll_after_seconds: Option<u64>,
}
#[derive(Deserialize)]
struct Options {
    method: String,
    configured: bool,
}
#[derive(Default)]
pub(super) struct Account {
    options: Load<Options>,
    view: Load<View>,
    next: Option<Instant>,
    apple: bool,
    avatar: avatar::Avatar,
    texture: Option<(String, Result<egui::TextureHandle, String>)>,
    challenge: Arc<Mutex<Challenge>>,
    owner: Option<(ServerSession, Handle, String)>,
}
#[derive(Default)]
struct Challenge {
    closed: bool,
    id: Option<String>,
}
async fn change(s: &ServerSession, path: &str, action: &str, id: &str) -> Result<View, String> {
    decode(
        s.settings_json(
            "POST",
            path,
            Some(&json!({"action":action,"loginId":id})),
            &[],
        )
        .await?,
    )
}
impl Drop for Account {
    fn drop(&mut self) {
        let Some((s, rt, path)) = self.owner.take() else {
            return;
        };
        let mut scope = self.challenge.lock().unwrap();
        scope.closed = true;
        if let Some(id) = scope.id.take() {
            rt.spawn(async move {
                let _ = change(&s, &path, "cancel", &id).await;
            });
        }
    }
}
impl Account {
    pub fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>, instance: &Value, refresh: bool) {
        let path = format!(
            "/online-instances/{}/account",
            path_id(text(instance, "instanceId"))
        );
        self.owner
            .get_or_insert_with(|| (svc.server.clone(), svc.runtime.clone(), path.clone()));
        self.options.poll();
        if self.options.initial() || self.options.status(ui) || refresh {
            let (s, p) = (svc.server.clone(), format!("{path}/options"));
            self.options
                .start(ui, svc, async move { get(&s, &p).await });
        }
        if self.view.poll() {
            let v = self.view.value.as_ref().unwrap();
            self.next = Some(
                Instant::now()
                    + Duration::from_secs(v.poll_after_seconds.unwrap_or(3).clamp(3, 300)),
            );
            if v.state == "signedIn" {
                self.apple = false;
            }
            if v.login_id.is_none() {
                self.texture = None;
            }
        }
        let mut action = None;
        let reload = self.view.status(ui) || self.view.initial() || refresh;
        let configured = self.options.value.as_ref().is_some_and(|o| o.configured);
        if let Some(view) = &self.view.value {
            ui.horizontal_wrapped(|ui| {
                if view.state == "signedIn" {
                    self.avatar.show(ui, svc, view.avatar_url.as_deref());
                }
                if let Some(name) = &view.display_name {
                    ui.strong(name);
                }
                ui.weak(tr(match view.state.as_str() {
                    "signedOut" => "未登录",
                    "waiting" => "等待授权",
                    "scanned" => "已扫码，请在手机确认",
                    "signedIn" => "已登录",
                    "unverified" => "登录状态待校验",
                    "expired" => "已过期，请重新登录",
                    "canceled" => "已取消登录",
                    other => other,
                }));
                if view.state == "signedIn" && text(instance, "platform") == "netease" {
                    let label = match view.membership_tier.as_deref() {
                        Some("free") => Some(tr("非会员")),
                        Some("vip") => Some("VIP"),
                        Some("svip") => Some("SVIP"),
                        _ => None,
                    };
                    if let Some(label) = label {
                        ui.weak(label);
                    }
                }
            });
            let bound = view.account_id.as_ref().is_some_and(|s| !s.is_empty());
            let pending = view.login_id.as_ref().is_some_and(|s| !s.is_empty());
            let offline = text(instance, "availability") == "offline";
            ui.add_enabled_ui(!self.view.busy(), |ui| {
                if bound {
                    ui.horizontal(|ui| {
                        if style::button(ui, tr("校验登录状态")).clicked() {
                            action = Some("check");
                        }
                        if style::button(ui, tr("退出平台账号")).clicked() {
                            action = Some("logout");
                        }
                    });
                } else if self
                    .options
                    .value
                    .as_ref()
                    .is_some_and(|o| o.method == "musicKit")
                {
                    if style::add_button(
                        ui,
                        configured && !offline,
                        egui::Button::new(tr("打开 Apple 授权")),
                    )
                    .clicked()
                    {
                        // 平台授权在同一服务端的官方 MusicKit 页面完成，凭据不会进入浏览器地址。
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
                            "{}/#/settings?tab=sources",
                            svc.server.info.endpoint
                        )));
                        self.apple = true;
                        self.next = Some(Instant::now() + Duration::from_secs(3));
                    }
                    if self.apple {
                        ui.weak(tr(
                            "请在浏览器中登录同一 MusicFS 账号并完成 Apple Music 授权。",
                        ));
                        if style::button(ui, tr("刷新账号状态")).clicked() {
                            action = Some("read");
                        }
                    }
                } else if !pending
                    && style::add_button(ui, configured && !offline, egui::Button::new(tr("登录")))
                        .clicked()
                {
                    action = Some("begin");
                }
                if pending && view.method.as_deref() == Some("qr") {
                    let id = view.login_id.as_deref().unwrap();
                    if self.texture.as_ref().is_none_or(|(k, _)| k != id) {
                        let result = qr(view).map(|pixels| {
                            ui.ctx().load_texture(
                                "platform-login-qr",
                                pixels,
                                egui::TextureOptions::NEAREST,
                            )
                        });
                        self.texture = Some((id.into(), result));
                    }
                    match &self.texture.as_ref().unwrap().1 {
                        Ok(t) => {
                            ui.image((t.id(), egui::vec2(224., 224.)));
                        }
                        Err(error) => {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    }
                    ui.weak(tr("请使用平台手机应用扫码并确认登录。"));
                    if style::button(ui, tr("刷新扫码状态")).clicked() {
                        action = Some("poll");
                    }
                }
                if pending && style::button(ui, tr("取消登录")).clicked() {
                    action = Some("cancel");
                }
            });
            if !self.view.busy()
                && self.view.error.is_none()
                && action.is_none()
                && self.next.is_some_and(|t| Instant::now() >= t)
            {
                if pending && view.method.as_deref() == Some("qr") {
                    action = Some("poll");
                } else if self.apple {
                    action = Some("read");
                }
            }
            if offline {
                ui.weak(tr("此音乐来源暂时离线，请稍后重试。"));
            }
            if self
                .options
                .value
                .as_ref()
                .is_some_and(|o| !o.configured && o.method == "musicKit")
            {
                ui.weak(tr("请先在服务端配置 Apple Music 开发者授权。"));
            }
        }
        if reload {
            action = Some("read");
        }
        if let Some(action) = action {
            let (s, scope) = (svc.server.clone(), self.challenge.clone());
            let id = self
                .view
                .value
                .as_ref()
                .and_then(|v| v.login_id.clone())
                .unwrap_or_default();
            self.view.start(ui, svc, async move {
                let v: View = if action == "read" {
                    get(&s, &path).await?
                } else {
                    change(&s, &path, action, &id).await?
                };
                let cancel = {
                    let mut scope = scope.lock().unwrap();
                    if scope.closed {
                        v.login_id.clone()
                    } else {
                        scope.id.clone_from(&v.login_id);
                        None
                    }
                };
                if let Some(id) = cancel {
                    let _ = change(&s, &path, "cancel", &id).await;
                }
                Ok(v)
            });
        }
    }
}

fn qr(view: &View) -> Result<egui::ColorImage, String> {
    if let Some(data) = view
        .qr_image
        .as_deref()
        .and_then(|s| s.strip_prefix("data:image/png;base64,"))
    {
        if data.len() > 2_000_000 {
            return Err("QR image is too large.".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| "Invalid QR image.")?;
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
        let (w, h) = reader.into_dimensions().map_err(|_| "Invalid QR image.")?;
        if w == 0 || h == 0 || w > 1024 || h > 1024 {
            return Err("Invalid QR image dimensions.".into());
        }
        let pixels = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|_| "Invalid QR image.")?
            .to_rgba8();
        return Ok(egui::ColorImage::from_rgba_unmultiplied(
            [w as usize, h as usize],
            pixels.as_raw(),
        ));
    }
    let url = view
        .qr_url
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Missing login QR code.")?;
    let code = QrCode::new(url.as_bytes()).map_err(|_| "Invalid QR payload.")?;
    let size = code.width() + 8;
    let mut result = egui::ColorImage::new([size, size], egui::Color32::WHITE);
    for y in 0..code.width() {
        for x in 0..code.width() {
            if code[(x, y)] == Color::Dark {
                result[(x + 4, y + 4)] = egui::Color32::BLACK;
            }
        }
    }
    Ok(result)
}
