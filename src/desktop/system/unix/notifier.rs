use super::{Menu, SystemAction, SystemSender, SystemState};
use ksni::{menu::StandardItem, Icon, MenuItem, Tray, TrayMethods};
use std::time::Duration;
use tokio::sync::watch;

struct Item {
    sender: SystemSender,
    menu: Menu,
    icon: Icon,
}

impl Tray for Item {
    fn id(&self) -> String {
        "MusicFS".into()
    }
    fn title(&self) -> String {
        "MusicFS".into()
    }
    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![self.icon.clone()]
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.sender.send(SystemAction::Show);
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.menu
            .entries()
            .into_iter()
            .map(|(label, action, enabled)| {
                StandardItem {
                    label: label.to_owned(),
                    enabled,
                    activate: Box::new(move |item: &mut Self| item.sender.send(action)),
                    ..Default::default()
                }
                .into()
            })
            .collect()
    }
}

fn icon() -> Icon {
    let image = image::load_from_memory(include_bytes!("../../../app/icons/128x128.png"))
        .expect("Invalid application icon")
        .thumbnail(32, 32)
        .to_rgba8();
    let (width, height) = image.dimensions();
    let mut data = image.into_vec();
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
    Icon {
        width: width as i32,
        height: height as i32,
        data,
    }
}

pub(super) async fn run(sender: SystemSender, mut states: watch::Receiver<Option<SystemState>>) {
    while states.borrow().is_none() {
        if states.changed().await.is_err() {
            return;
        }
    }
    let icon = icon();
    loop {
        let Some(mut menu) = states.borrow_and_update().as_ref().map(Menu::from) else {
            return;
        };
        let item = Item {
            sender: sender.clone(),
            menu: menu.clone(),
            icon: icon.clone(),
        };
        if let Ok(handle) = item.assume_sni_available(true).spawn().await {
            loop {
                if states.changed().await.is_err() {
                    handle.shutdown().await;
                    return;
                }
                let Some(next) = states.borrow_and_update().as_ref().map(Menu::from) else {
                    continue;
                };
                if menu != next {
                    menu = next.clone();
                    if handle.update(move |item| item.menu = next).await.is_none() {
                        break;
                    }
                } else if handle.is_closed() {
                    break;
                }
            }
        }
        if states.has_changed().is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
