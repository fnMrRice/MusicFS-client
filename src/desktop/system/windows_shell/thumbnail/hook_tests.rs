use super::*;
use crate::system::{MediaTrack, SystemAction};
use std::{
    ptr::null_mut,
    sync::{
        atomic::AtomicU64,
        mpsc::{sync_channel, Receiver},
        Arc,
    },
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

struct TestWindow(HWND);
impl TestWindow {
    fn new() -> Self {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                wide("STATIC").as_ptr(),
                wide("MusicFS thumbnail test").as_ptr(),
                WS_POPUP,
                0,
                0,
                80,
                80,
                null_mut(),
                null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        Self(hwnd)
    }
    fn buttons(&self) -> (Thumbnail, Receiver<(u64, SystemAction)>) {
        let (tx, rx) = sync_channel(4);
        let sender = SystemSender {
            tx,
            epoch: Arc::new(AtomicU64::new(17)),
            ctx: Default::default(),
        };
        (Thumbnail::new(self.0 as isize, sender).unwrap(), rx)
    }
    fn command(&self, notification: u32, id: u32) {
        unsafe {
            SendMessageW(
                self.0,
                WM_COMMAND,
                ((notification as usize) << 16) | id as usize,
                0,
            );
        }
    }
}
impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}

#[test]
fn native_thumbnail_commands_are_scoped_to_the_attached_window_and_detached_on_exit() {
    use windows::Win32::UI::Shell::THBN_CLICKED;
    let first = TestWindow::new();
    let second = TestWindow::new();
    let (controls, rx) = first.buttons();
    let (other, other_rx) = second.buttons();
    let generation = controls.0.generation.get();
    unsafe {
        SendMessageW(first.0, controls.0.created, 0, 0);
    }
    assert_eq!(controls.0.generation.get(), generation + 1);
    assert_eq!(other.0.generation.get(), generation);
    let mut state = SystemState::default();
    state.track = Some(MediaTrack {
        id: "track".into(),
        title: "Song".into(),
        artists: vec![],
        album: None,
        artwork_png: None,
    });
    state.can_next = true;
    controls.update(&state);
    first.command(0, 0x4d52);
    first.command(THBN_CLICKED, 0x4d51);
    assert!(rx.try_recv().is_err());
    first.command(THBN_CLICKED, 0x4d52);
    first.command(THBN_CLICKED, 0x4d53);
    assert_eq!(rx.try_recv().unwrap(), (17, SystemAction::Play));
    assert_eq!(rx.try_recv().unwrap(), (17, SystemAction::Next));
    assert!(other_rx.try_recv().is_err());
    let released = Rc::downgrade(&controls.0);
    drop(controls);
    assert!(released.upgrade().is_none());
    first.command(THBN_CLICKED, 0x4d52);
    drop(second);
    assert!(other.0.hwnd.get().is_null());
    assert_eq!(Rc::strong_count(&other.0), 1);
}
