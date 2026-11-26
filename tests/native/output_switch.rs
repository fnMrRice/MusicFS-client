use super::{test_audio_frames, track};
use crate::{output_devices, FadeSettings, Player, PlayerCommand, PlayerSnapshot, Status};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_for(player: &Player, ready: impl Fn(&PlayerSnapshot) -> bool) -> PlayerSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = player.snapshot().unwrap();
        assert_ne!(state.status, Status::Error, "{:?}", state.error);
        if ready(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "Timed out waiting for playback");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "requires an audio output device"]
fn switch_output_preserves_a_nonseekable_session_and_pause() {
    let devices = output_devices().unwrap();
    let default = devices
        .iter()
        .find(|d| d.is_default)
        .expect("No default output");
    let target = devices
        .iter()
        .find(|d| !d.is_default && d.max_channels.is_some())
        .unwrap_or(default);
    eprintln!(
        "Output devices: {}. Switching from {} to {}.",
        devices.len(),
        default.name,
        target.name
    );
    let opens = Arc::new(AtomicUsize::new(0));
    let observed = opens.clone();
    let player = Player::new(move |_| {
        observed.fetch_add(1, Ordering::Relaxed);
        let mut media = test_audio_frames(8000 * 20, || {});
        media.seekable = false;
        media.seek_range_ms = None;
        Ok(media)
    })
    .unwrap();
    let set_device = |device: Option<&str>| {
        player.command(PlayerCommand::OutputDevice {
            device: device.map(str::to_owned),
        })
    };
    player
        .command(PlayerCommand::Volume { volume: 0. })
        .unwrap();
    player
        .command(PlayerCommand::SetFades {
            settings: FadeSettings {
                enabled: false,
                ..Default::default()
            },
        })
        .unwrap();
    player.set_queue(vec![track("output-test")], 0).unwrap();
    let before = wait_for(&player, |s| {
        s.status == Status::Playing && s.position_ms > 100
    });
    assert!(!before.seekable);
    eprintln!("Playing on the default device.");
    assert!(set_device(Some("missing-device-for-test")).is_err());
    let switched = set_device(Some(&target.id)).unwrap();
    assert_eq!(switched.status, Status::Playing);
    assert_eq!(switched.index, before.index);
    wait_for(&player, |s| s.position_ms > before.position_ms + 200);
    eprintln!("Playing on the selected device.");
    player.command(PlayerCommand::Pause).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let paused_at = player.snapshot().unwrap().position_ms;
    let switched = set_device(None).unwrap();
    assert_eq!(switched.status, Status::Paused);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(player.snapshot().unwrap().position_ms, paused_at);
    player.command(PlayerCommand::Play).unwrap();
    let resumed = wait_for(&player, |s| s.position_ms > paused_at + 100);
    assert_eq!(resumed.volume, 0.);
    assert_eq!(opens.load(Ordering::Relaxed), 1);
    player.command(PlayerCommand::Stop).unwrap();
}
