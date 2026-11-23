use super::{mac_menu::StatusMenu, now_playing::NowPlaying, SystemSender, SystemState};

pub(crate) struct UnixDesktop {
    media: NowPlaying,
    menu: Option<StatusMenu>,
}

impl UnixDesktop {
    pub(crate) fn new(sender: SystemSender) -> Result<Self, String> {
        objc2::MainThreadMarker::new().ok_or("The macOS desktop requires the main thread.")?;
        let media = NowPlaying::new(sender.clone());
        let menu = StatusMenu::new(sender)
            .map_err(|e| eprintln!("MusicFS status menu: {e}"))
            .ok();
        Ok(Self { media, menu })
    }

    pub(crate) fn update(&mut self, state: &SystemState) {
        self.media.update(state);
        if let Some(menu) = &mut self.menu {
            menu.update(state);
        }
    }
}
