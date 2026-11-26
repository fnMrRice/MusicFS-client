use super::*;
use crate::tests::track;
use std::collections::HashSet;
use std::time::{Duration, Instant};

fn queue(ids: &[&str], start: usize, mode: PlaybackMode) -> Actor {
    let mut actor = Actor::new(Worker::new(|_| Err("Unused test loader".into())).unwrap());
    actor
        .apply(Operation::Queue(
            ids.iter().map(|id| track(id)).collect(),
            start,
        ))
        .unwrap();
    actor
        .command(PlayerCommand::SetPlaybackMode { mode })
        .unwrap();
    actor
}

#[test]
fn natural_end_and_manual_skips_follow_the_selected_mode() {
    for mode in [
        PlaybackMode::Sequence,
        PlaybackMode::RepeatOne,
        PlaybackMode::RepeatAll,
    ] {
        let mut actor = queue(&["a", "b"], 0, mode);
        assert_eq!(
            actor.snapshot().can_previous,
            mode == PlaybackMode::RepeatAll
        );
        assert!(actor.snapshot().can_next);
        actor.advance(true).unwrap();
        if mode == PlaybackMode::RepeatOne {
            assert_eq!(actor.state.index, Some(0));
            actor.command(PlayerCommand::Next).unwrap();
        }
        assert_eq!(actor.state.index, Some(1));
        let wraps = mode == PlaybackMode::RepeatAll;
        assert_eq!(actor.snapshot().can_next, wraps);
        assert_eq!(actor.presentation().has_next, wraps);
        assert_eq!(actor.track_tail_ms() > 0, mode != PlaybackMode::Sequence);
        actor.advance(true).unwrap();
        assert_eq!(actor.state.index, Some(if wraps { 0 } else { 1 }));
        assert_eq!(
            actor.state.status == Status::Ended,
            mode == PlaybackMode::Sequence
        );
        actor.command(PlayerCommand::Previous).unwrap();
        assert_eq!(actor.state.index, Some(if wraps { 1 } else { 0 }));
    }
}

#[test]
fn shuffle_visits_every_queue_entry_and_retraces_history_after_an_insertion() {
    // 重复录音也是不同的队列条目。
    let mut actor = queue(&["a", "b", "a", "c", "d"], 2, PlaybackMode::Shuffle);
    let initial = 2;
    assert!(!actor.presentation().has_previous);
    let mut visited = HashSet::from([initial]);
    for _ in 1..5 {
        actor.advance(true).unwrap();
        assert!(visited.insert(actor.state.index.unwrap()));
    }
    let last = actor.state.index.unwrap();
    actor.advance(true).unwrap();
    let first = actor.state.index.unwrap();
    assert_ne!(first, last);
    visited = HashSet::from([first]);
    for _ in 1..5 {
        actor.command(PlayerCommand::Next).unwrap();
        assert!(visited.insert(actor.state.index.unwrap()));
    }

    let latest = actor.state.index.unwrap();
    actor.command(PlayerCommand::Previous).unwrap();
    let previous = actor.state.index.unwrap();
    actor
        .apply(Operation::Enqueue {
            tracks: vec![track("priority")],
            next: true,
        })
        .unwrap();
    actor.advance(true).unwrap();
    assert_eq!(actor.state.queue[actor.state.index.unwrap()].id, "priority");
    actor.command(PlayerCommand::Next).unwrap();
    assert_eq!(
        actor.state.index,
        Some(latest + usize::from(latest > previous))
    );
    assert_eq!(
        actor.presentation().has_previous,
        actor.snapshot().can_previous
    );
}

#[test]
fn play_next_wins_over_repetition_and_clear_keeps_the_preference() {
    let mut actor = queue(&["a"], 0, PlaybackMode::RepeatOne);
    actor
        .apply(Operation::Enqueue {
            tracks: vec![track("b"), track("c")],
            next: true,
        })
        .unwrap();
    for expected in [1, 2, 2] {
        actor.advance(true).unwrap();
        assert_eq!(actor.state.index, Some(expected));
    }
    let playing = actor.generation;
    actor
        .command(PlayerCommand::SetPlaybackMode {
            mode: PlaybackMode::Shuffle,
        })
        .unwrap();
    assert_eq!(actor.state.index, Some(2));
    assert_eq!(actor.generation, playing);
    assert!(!actor.snapshot().can_previous);
    actor.command(PlayerCommand::Clear).unwrap();
    assert_eq!(actor.snapshot().playback_mode, PlaybackMode::Shuffle);
    assert!(!actor.snapshot().can_next);
    actor
        .apply(Operation::Enqueue {
            tracks: vec![track("only")],
            next: false,
        })
        .unwrap();
    actor.advance(true).unwrap();
    assert_eq!(actor.state.index, Some(0));
    assert_eq!(actor.state.status, Status::Loading);
    assert!(!actor.presentation().has_next);
    actor
        .apply(Operation::Queue(
            vec![track("replacement"), track("last")],
            1,
        ))
        .unwrap();
    assert!(!actor.snapshot().can_previous);
    actor.command(PlayerCommand::Next).unwrap();
    assert_eq!(actor.state.index, Some(0));
}

#[test]
#[ignore = "requires a system default audio output device"]
fn silent_wav_ends_on_the_default_output_in_each_mode() {
    use crate::{tests::test_audio, Player};
    for mode in [
        PlaybackMode::Sequence,
        PlaybackMode::RepeatOne,
        PlaybackMode::RepeatAll,
        PlaybackMode::Shuffle,
    ] {
        let (audible, events) = std::sync::mpsc::channel();
        let player = Player::new(move |track| {
            let event = audible.clone();
            let id = track.id.clone();
            let mut media = test_audio(|| {});
            media.on_started = Box::new(move || {
                let _ = event.send(id);
            });
            Ok(media)
        })
        .unwrap();
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
        player
            .command(PlayerCommand::SetPlaybackMode { mode })
            .unwrap();
        let start = if mode == PlaybackMode::RepeatAll {
            2
        } else {
            0
        };
        player
            .set_queue(["a", "b", "c"].iter().map(|id| track(id)).collect(), start)
            .unwrap();
        let mut played = Vec::new();
        let count = match mode {
            PlaybackMode::Sequence | PlaybackMode::RepeatOne => 3,
            PlaybackMode::RepeatAll => 4,
            PlaybackMode::Shuffle => 6,
        };
        let deadline = Instant::now() + Duration::from_secs(12);
        while played.len() < count {
            if let Ok(id) = events.recv_timeout(Duration::from_millis(50)) {
                played.push(id);
            }
            let snapshot = player.snapshot().unwrap();
            assert_ne!(
                snapshot.status,
                Status::Error,
                "{mode:?}: {:?}",
                snapshot.error
            );
            assert!(
                Instant::now() < deadline,
                "{mode:?} timed out after {played:?}"
            );
        }
        match mode {
            PlaybackMode::Sequence => {
                assert_eq!(played, ["a", "b", "c"]);
                while player.snapshot().unwrap().status != Status::Ended {
                    assert!(Instant::now() < deadline, "Sequence did not stop");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            PlaybackMode::RepeatOne => assert_eq!(played, ["a", "a", "a"]),
            PlaybackMode::RepeatAll => assert_eq!(played, ["c", "a", "b", "c"]),
            PlaybackMode::Shuffle => {
                assert_eq!(played[..3].iter().collect::<HashSet<_>>().len(), 3);
                assert_eq!(played[3..].iter().collect::<HashSet<_>>().len(), 3);
                assert_ne!(played[2], played[3]);
            }
        }
        player.command(PlayerCommand::Stop).unwrap();
        eprintln!("{mode:?} on the default audio output: {played:?}");
    }
}
