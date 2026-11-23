use super::*;
use crate::pcm::{Block, PcmSource};
use crate::tests::track;

#[test]
fn seek_and_track_changes_follow_their_own_settings() {
    for seek in [true, false] {
        for track_change in [true, false] {
            let mut actor = Actor::new(Worker::new(|_| Err("Unused test loader".into())).unwrap());
            actor
                .command(PlayerCommand::SetFades {
                    settings: FadeSettings { seek, track_change },
                })
                .unwrap();
            actor
                .apply(Operation::Queue(
                    vec![track("a"), track("b"), track("c")],
                    0,
                ))
                .unwrap();
            assert!(!actor.fade_in);
            // 在无设备测试中从就绪的定位通道开始，保留接收端直到命令完成。
            let (sender, _receiver) = std::sync::mpsc::channel();
            actor.seeks = Some(sender);
            actor.state.seekable = true;
            actor.state.seek_range_ms = Some((0, 100));
            actor.seek(30).unwrap();
            assert_eq!(actor.fade_in, seek);
            actor.pause();
            actor.seek(40).unwrap();
            assert!(!actor.fade_in);
            assert_eq!(actor.state.status, Status::Paused);
            actor.command(PlayerCommand::Next).unwrap();
            assert_eq!(actor.fade_in, track_change);
            actor.advance(true).unwrap();
            assert_eq!(actor.fade_in, track_change);
            actor.advance(true).unwrap();
            assert!(!actor.fade_in);
            assert_eq!(actor.state.status, Status::Ended);
        }
    }
}

fn samples(frames: usize) -> Block {
    let mut block = Block::empty();
    block.len = frames * 2;
    block.data[..block.len].fill(1.0);
    block
}

fn read_frame(source: &mut PcmSource) -> f32 {
    let left = source.next().unwrap();
    assert_eq!(source.next(), Some(left));
    left
}

#[test]
fn buffering_does_not_consume_fade_in_and_retiring_fades_from_current_gain() {
    let (mut writer, mut source, clock) = PcmSource::pipe(2, 100, 5000);
    source.envelope.fade_in = true;
    for _ in 0..50 {
        assert_eq!(read_frame(&mut source), 0.0);
    }
    assert_eq!(clock.position_ms(), 5000);
    writer.try_send(samples(55)).unwrap();
    assert_eq!(read_frame(&mut source), 0.0);
    for _ in 1..15 {
        read_frame(&mut source);
    }
    clock.fades.retiring.store(true, Ordering::Release);
    let mut previous = read_frame(&mut source);
    assert!((previous - 15.0 / 29.0).abs() < 0.0001);
    for _ in 1..30 {
        let gain = read_frame(&mut source);
        assert!(gain < previous);
        previous = gain;
    }
    assert_eq!(previous, 0.0);
    assert_eq!(source.next(), None);
    assert_eq!(clock.position_ms(), 5450);
}

#[test]
fn natural_change_fades_only_the_last_frames_of_a_track_with_a_successor() {
    for track_tail in [true, false] {
        let (mut writer, mut source, clock) = PcmSource::pipe(2, 100, 0);
        clock.fades.track_tail.store(track_tail, Ordering::Release);
        writer.try_send(samples(50)).unwrap();
        drop(writer);
        for frame in 0..50 {
            let expected = if track_tail && frame >= 20 {
                (49 - frame) as f32 / 29.0
            } else {
                1.0
            };
            assert_eq!(read_frame(&mut source), expected);
        }
        assert_eq!(source.next(), None);
    }
}
