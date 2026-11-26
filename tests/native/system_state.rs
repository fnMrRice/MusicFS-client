use super::*;
use musicfs_player_core::Track;

fn fixture() -> PlayerSnapshot {
    let track = Track {
        id: "same".into(),
        title: "Title".into(),
        artists: vec!["A".into(), "B".into()],
        album: Some("Album".into()),
        source_only: false,
        source_track_id: None,
        forced_source_track_id: None,
        cover_object_id: None,
        online_cover_path: None,
        duration_ms: Some(9_000),
    };
    PlayerSnapshot {
        queue: Arc::new(vec![track.clone(), track]),
        index: Some(0),
        can_next: true,
        can_previous: false,
        position_ms: 3_000,
        status: Status::Playing,
        seekable: true,
        seek_range_ms: Some((2_000, 5_000)),
        ..Default::default()
    }
}

#[test]
fn queue_entries_and_sessions_have_independent_system_identities() {
    let mut snapshot = fixture();
    let first = SystemState::from_snapshot(1, &snapshot, None);
    assert!(first.can_next);
    assert_eq!(first.duration_ms, Some(9_000));
    assert_eq!(first.track.as_ref().unwrap().artists, ["A", "B"]);
    assert_ne!(
        first.track_id(),
        SystemState::from_snapshot(2, &snapshot, None).track_id()
    );
    snapshot.index = Some(1);
    snapshot.can_next = false;
    snapshot.can_previous = true;
    let last = SystemState::from_snapshot(1, &snapshot, None);
    assert_ne!(first.track_id(), last.track_id());
    assert!(!last.can_next);
    assert!(last.command(SystemAction::Next).is_none());
    snapshot.can_next = true;
    let looping = SystemState::from_snapshot(1, &snapshot, None);
    assert!(matches!(
        looping.command(SystemAction::Next),
        Some(PlayerCommand::Next)
    ));
    snapshot.index = None;
    assert_eq!(
        SystemState::from_snapshot(1, &snapshot, None),
        SystemState::default()
    );
}

#[test]
fn media_commands_respect_the_current_stream_and_transport_state() {
    let mut snapshot = fixture();
    let state = SystemState::from_snapshot(1, &snapshot, None);
    assert!(state.command(SystemAction::Play).is_none());
    assert!(matches!(
        state.command(SystemAction::Pause),
        Some(PlayerCommand::Pause)
    ));
    for (action, target) in [
        (SystemAction::SeekBy(i64::MIN), 2_000),
        (SystemAction::SeekBy(300), 3_300),
        (SystemAction::Seek(u64::MAX), 5_000),
    ] {
        assert!(
            matches!(state.command(action),Some(PlayerCommand::Seek{position_ms}) if position_ms==target)
        );
    }
    snapshot.status = Status::Paused;
    snapshot.seekable = false;
    let state = SystemState::from_snapshot(1, &snapshot, None);
    assert!(state.command(SystemAction::Pause).is_none());
    assert!(matches!(
        state.command(SystemAction::Play),
        Some(PlayerCommand::Play)
    ));
    assert!(state.command(SystemAction::Seek(3_000)).is_none());
    assert!(SystemState::default()
        .command(SystemAction::Toggle)
        .is_none());
}
