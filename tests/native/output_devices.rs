use super::*;
use crate::pcm::Block;
use rodio::mixer::mixer;

#[test]
fn mixer_releases_active_and_paused_sources_for_device_switch() {
    let settings = Processing::default();
    for paused in [false, true] {
        let (mut producer, pcm, clock) = PcmSource::pipe(1, 48000, 0);
        let mut block = Block::empty();
        block.data.fill(0.5);
        block.len = block.data.len();
        assert!(producer.try_send(block).is_ok());
        let (source, _) = RenderSource::new(pcm, 2, &settings, paused);
        let (device, mut output) = mixer(2, 48000);
        let sink = Sink::connect_new(&device);
        if paused {
            sink.pause();
        }
        let receiver = StreamSource::append(&sink, Box::new(source));
        for _ in 0..2048 {
            output.next();
        }
        assert_eq!(clock.started(), !paused);
        drop(device);
        drop(output);
        drop(sink);
        let mut source = receiver.recv_timeout(Duration::from_millis(100)).unwrap();
        let position = clock.position_ms();
        source.change_device(1, &settings, paused);
        assert_eq!(source.channels(), 1);
        let samples: Vec<_> = source.take(96).collect();
        assert_eq!(samples.len(), 96);
        if paused {
            assert!(samples.iter().all(|sample| *sample == 0.));
            assert_eq!(clock.position_ms(), position);
        } else {
            assert!(samples.iter().all(|sample| *sample > 0.));
            assert_eq!(clock.position_ms(), position + 2);
        }
    }
}
