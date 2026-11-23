use super::{Menu, SystemAction, SystemSender, SystemState};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tray_icon::{
    menu::{Menu as NativeMenu, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

const IDS: [&str; 6] = [
    "musicfs.show",
    "musicfs.previous",
    "musicfs.toggle",
    "musicfs.next",
    "musicfs.stop",
    "musicfs.quit",
];

fn register(sender: &Arc<SystemSender>) {
    static TARGET: OnceLock<Mutex<Weak<SystemSender>>> = OnceLock::new();
    let target = TARGET.get_or_init(|| {
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let Some(sender) = TARGET.get().and_then(|s| s.lock().ok()?.upgrade()) else {
                return;
            };
            let action = match event.id.as_ref() {
                "musicfs.show" => SystemAction::Show,
                "musicfs.previous" => SystemAction::Previous,
                "musicfs.toggle" => SystemAction::Toggle,
                "musicfs.next" => SystemAction::Next,
                "musicfs.stop" => SystemAction::Stop,
                "musicfs.quit" => SystemAction::Quit,
                _ => return,
            };
            sender.send(action);
        }));
        Mutex::new(Weak::new())
    });
    if let Ok(mut slot) = target.lock() {
        *slot = Arc::downgrade(sender);
    }
}

pub(super) struct StatusMenu {
    _tray: TrayIcon,
    _sender: Arc<SystemSender>,
    items: Vec<MenuItem>,
    last: Menu,
}

impl StatusMenu {
    pub(super) fn new(sender: SystemSender) -> Result<Self, String> {
        let menu = NativeMenu::new();
        let last = Menu::from(&SystemState::default());
        let mut items = Vec::new();
        for (id, (text, _, enabled)) in IDS.into_iter().zip(last.entries()) {
            let item = MenuItem::with_id(id, text, enabled, None);
            menu.append(&item).map_err(|e| e.to_string())?;
            items.push(item);
        }
        let image = image::load_from_memory(include_bytes!("../../../app/icons/128x128.png"))
            .map_err(|e| e.to_string())?
            .thumbnail(36, 36)
            .to_rgba8();
        let (w, h) = image.dimensions();
        let icon = Icon::from_rgba(image.into_raw(), w, h).map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("MusicFS")
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;
        let sender = Arc::new(sender);
        register(&sender);
        Ok(Self {
            _tray: tray,
            _sender: sender,
            items,
            last,
        })
    }

    pub(super) fn update(&mut self, state: &SystemState) {
        let current = Menu::from(state);
        if self.last == current {
            return;
        }
        for (item, (text, _, enabled)) in self.items.iter().zip(current.entries()) {
            item.set_text(text);
            item.set_enabled(enabled);
        }
        self.last = current;
    }
}
