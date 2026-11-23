use super::*;
use crate::decode::{Event, Worker};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

pub(crate) fn track(id: &str) -> Track {
    Track {
        id: id.into(),
        source_only: false,
        title: id.into(),
        artists: vec![],
        album: None,
        source_track_id: Some(id.into()),
        forced_source_track_id: None,
        cover_object_id: None,
        duration_ms: Some(100),
    }
}

fn test_audio(on_close: impl FnOnce() + Send + 'static) -> Media {
    let rate: u32 = 8000;
    let samples: u32 = rate / 10;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(samples * 2).to_le_bytes());
    for _ in 0..samples {
        wav.extend_from_slice(&4096i16.to_le_bytes());
    }
    Media {
        source_track_id: "test".into(),
        seek_range_ms: Some((0, 100)),
        byte_length: Some(wav.len() as u64),
        reader: Box::new(Cursor::new(wav)),
        seekable: true,
        on_started: Box::new(|| {}),
        on_close: Box::new(on_close),
    }
}

#[test]
fn decodes_and_seeks_wav_without_a_device() {
    let closed = Arc::new(AtomicUsize::new(0));
    let observed = closed.clone();
    let worker = Worker::new(move |_| {
        let observed = observed.clone();
        Ok(test_audio(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        }))
    })
    .unwrap();
    let generation = worker.cancel();
    let seek = worker.load(generation, track("a")).unwrap();
    let receive = || {
        worker
            .events
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .1
    };
    let Event::Opened(lease) = receive() else {
        panic!("Missing lease")
    };
    let Event::Ready {
        mut source,
        clock,
        seekable,
        ..
    } = receive()
    else {
        panic!("Missing PCM")
    };
    assert!(seekable);
    assert_eq!(clock.position_ms(), 0);
    for _ in 0..400 {
        assert!(source.next().unwrap() > 0.0);
    }
    assert_eq!(clock.position_ms(), 50);
    drop(source);
    seek.send((1, 70)).unwrap();
    let Event::Ready {
        revision,
        mut source,
        clock,
        ..
    } = receive()
    else {
        panic!("Seek failed")
    };
    assert_eq!(revision, 1);
    assert_eq!(clock.position_ms(), 70);
    assert!(source.next().unwrap() > 0.0);
    worker.cancel();
    drop(lease);
    assert_eq!(closed.load(Ordering::SeqCst), 1);
}

#[test]
fn clear_invalidates_a_blocked_load_and_closes_its_late_lease() {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let wait_gate = gate.clone();
    let (entered, loading) = mpsc::channel();
    let (closed, release) = mpsc::channel();
    let player = Player::new(move |_| {
        entered.send(()).unwrap();
        let (lock, changed) = &*wait_gate;
        let _opened = changed
            .wait_while(lock.lock().unwrap(), |open| !*open)
            .unwrap();
        let closed = closed.clone();
        Ok(test_audio(move || {
            let _ = closed.send(());
        }))
    })
    .unwrap();
    let snapshot = player.set_queue(vec![track("a")], 0).unwrap();
    assert_eq!(snapshot.status, Status::Loading);
    loading.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(player.command(PlayerCommand::Select { index: 1 }).is_err());
    assert!(player
        .command(PlayerCommand::Volume { volume: 2.0 })
        .is_err());
    let paused = player.command(PlayerCommand::Pause).unwrap();
    assert_eq!(paused.status, Status::Paused);
    assert!(Arc::ptr_eq(&snapshot.queue, &paused.queue));
    let cleared = player.command(PlayerCommand::Clear).unwrap();
    assert!(cleared.queue.is_empty());
    assert_eq!(cleared.index, None);
    assert_eq!(snapshot.queue[0].id, "a");
    *gate.0.lock().unwrap() = true;
    gate.1.notify_one();
    release.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(player.snapshot().unwrap().status, Status::Idle);
}
#[test]
fn enqueue_preserves_idle_or_active_selection() {
    let player = crate::Player::new(|_| Err("Unused test loader".into())).unwrap();
    let first = player.enqueue(track("first")).unwrap();
    assert_eq!(first.status, crate::Status::Idle);
    assert_eq!(first.index, Some(0));
    let second = player.enqueue(track("second")).unwrap();
    assert_eq!(second.index, first.index);
    assert_eq!(second.queue.len(), 2);
    assert_eq!(first.queue.len(), 1);
    player.play_next(track("next")).unwrap();
    player
        .enqueue_many(vec![track("third"), track("fourth")])
        .unwrap();
    assert!(player
        .enqueue_many(vec![track("discard"), track("")])
        .is_err());
    let snapshot = player.snapshot().unwrap();
    assert_eq!(snapshot.index, first.index);
    assert_eq!(snapshot.status, first.status);
    assert_eq!(
        snapshot
            .queue
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["first", "next", "second", "third", "fourth"]
    );
}
