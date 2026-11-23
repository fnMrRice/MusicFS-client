mod state;
pub(crate) use state::{MediaTrack, SystemAction, SystemState};
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix;
#[cfg(windows)]
mod windows_media;
#[cfg(windows)]
mod windows_shell;

use crate::model::DesktopState;
use eframe::egui::{Context, ViewportCommand, ViewportId};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct SystemSender {
    tx: SyncSender<(u64, SystemAction)>,
    epoch: Arc<AtomicU64>,
    ctx: Context,
}

impl SystemSender {
    pub(crate) fn send(&self, action: SystemAction) {
        if self
            .tx
            .try_send((self.epoch.load(Ordering::Acquire), action))
            .is_ok()
        {
            self.ctx.request_repaint_of(ViewportId::ROOT);
        }
    }
}

pub(crate) struct DesktopSystem {
    rx: Receiver<(u64, SystemAction)>,
    epoch: Arc<AtomicU64>,
    last: Option<SystemState>,
    last_update: Instant,
    #[cfg(windows)]
    thumbnail: Option<windows_shell::Thumbnail>,
    #[cfg(windows)]
    shell: Option<windows_shell::WindowsShell>,
    #[cfg(windows)]
    media: Option<windows_media::WindowsMedia>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    desktop: Option<unix::UnixDesktop>,
}

fn available<T>(result: Result<T, String>) -> Option<T> {
    result
        .map_err(|e| eprintln!("MusicFS desktop integration: {e}"))
        .ok()
}

impl DesktopSystem {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = sync_channel(64);
        let epoch = Arc::new(AtomicU64::new(0));
        let sender = SystemSender {
            tx,
            epoch: epoch.clone(),
            ctx: creation.egui_ctx.clone(),
        };
        #[cfg(windows)]
        let hwnd = {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            creation
                .window_handle()
                .ok()
                .and_then(|handle| match handle.as_raw() {
                    RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
                    _ => None,
                })
        };
        Self {
            rx,
            epoch,
            last: None,
            last_update: Instant::now(),
            #[cfg(windows)]
            thumbnail: hwnd
                .and_then(|hwnd| available(windows_shell::Thumbnail::new(hwnd, sender.clone()))),
            #[cfg(windows)]
            shell: hwnd
                .and_then(|hwnd| available(windows_shell::WindowsShell::new(hwnd, sender.clone()))),
            #[cfg(windows)]
            media: hwnd.and_then(|hwnd| available(windows_media::WindowsMedia::new(hwnd, sender))),
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            desktop: available(unix::UnixDesktop::new(sender)),
        }
    }

    pub fn update(
        &mut self,
        ctx: &Context,
        desktop: &mut DesktopState,
        current: SystemState,
    ) -> bool {
        if self.last.as_ref() != Some(&current)
            || self.last_update.elapsed() >= Duration::from_secs(2)
        {
            self.last_update = Instant::now();
            if self.last.as_ref().and_then(SystemState::track_id) != current.track_id() {
                self.epoch.fetch_add(1, Ordering::AcqRel);
            }
            #[cfg(windows)]
            {
                if let Some(thumbnail) = &mut self.thumbnail {
                    thumbnail.update(&current);
                }
                if let Some(shell) = &mut self.shell {
                    shell.update(&current);
                }
                if let Some(media) = &mut self.media {
                    media.update(&current);
                }
            }
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if let Some(platform) = &mut self.desktop {
                platform.update(&current);
            }
            self.last = Some(current);
        }
        let mut changed = false;
        for (epoch, action) in self.rx.try_iter().take(64) {
            match action {
                SystemAction::Show => {
                    for command in [
                        ViewportCommand::Visible(true),
                        ViewportCommand::Minimized(false),
                        ViewportCommand::Focus,
                    ] {
                        ctx.send_viewport_cmd_to(ViewportId::ROOT, command);
                    }
                }
                SystemAction::Quit => {
                    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close)
                }
                #[cfg(target_os = "linux")]
                SystemAction::SetVolume(percent) if self.epoch.load(Ordering::Acquire) == epoch => {
                    if let Some(connection) = &desktop.connection {
                        let volume = f32::from(percent.min(100)) / 100.;
                        if desktop.prefs.audio.volume != volume {
                            match desktop
                                .prefs
                                .audio
                                .apply(connection, crate::model::audio::AudioAction::Volume(volume))
                            {
                                Ok(()) => {
                                    desktop.save_preferences();
                                    changed = true;
                                }
                                Err(error) => desktop.error = Some(error),
                            }
                        }
                    }
                }
                _ if self.epoch.load(Ordering::Acquire) == epoch => {
                    if let (Some(connection), Some(published)) = (&desktop.connection, &self.last) {
                        match published.dispatch(&connection.player, action) {
                            Ok(applied) => changed |= applied,
                            Err(err) => desktop.error = Some(err),
                        }
                    }
                }
                _ => {}
            }
        }
        changed
    }

    pub fn shutdown(&mut self) {
        #[cfg(windows)]
        {
            self.thumbnail.take();
            self.media.take();
            self.shell.take();
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.desktop.take();
    }
}

#[cfg(windows)]
pub(crate) fn handle_activation() -> bool {
    windows_shell::handle_activation()
}
