use super::*;
use crate::tests::{test_audio, track};

#[test]
fn playing_format_belongs_to_the_current_source() {
    let worker = Worker::new(|track| {
        let mut media = test_audio(|| {});
        media.source_track_id = track.id.clone();
        if track.id == "second" {
            media.format.bitrate_bps = None;
        }
        Ok(media)
    })
    .unwrap();
    let mut actor = Actor::new(worker);
    actor
        .apply(Operation::Queue(vec![track("first"), track("second")], 0))
        .unwrap();
    for (index, source, bitrate) in [(0, "first", Some(128000)), (1, "second", None)] {
        if index > 0 {
            actor.select(index).unwrap();
        }
        assert!(actor.state.active_format.is_none());
        // 只消费本代播放器的打开事件，无须创建系统音频设备。
        loop {
            let (generation, event) = actor
                .decoder
                .events
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            if generation == actor.generation {
                assert!(matches!(event, Event::Opened(_)));
                actor.handle(event).unwrap();
                break;
            }
        }
        assert_eq!(actor.state.active_source_track_id.as_deref(), Some(source));
        let format = actor.state.active_format.as_ref().unwrap();
        assert_eq!(format.codec.as_deref(), Some("pcm_s16le"));
        assert_eq!(format.bitrate_bps, bitrate);
    }
    actor.command(PlayerCommand::Stop).unwrap();
    assert!(actor.state.active_source_track_id.is_none());
    assert!(actor.state.active_format.is_none());
}
