use super::*;
use crate::audio::{Effect, EffectKind, Layout, LocalDsp, Routing};
use crate::pcm::{Block, PcmSource};
use std::f32::consts::TAU;

fn samples(kind: EffectKind, values: &[f32], hz: f32) -> Vec<f32> {
    let mut effect = Effect::new(kind);
    effect.values.copy_from_slice(values);
    let settings = Processing {
        dsp: LocalDsp {
            enabled: true,
            chain: vec![effect],
            ..Default::default()
        },
        ..Default::default()
    };
    let mut graph = Graph::new(&settings, 1, 1, 48000);
    (0..24000)
        .map(|i| {
            let mut out = [0.];
            graph.apply(&mut [(TAU * hz * i as f32 / 48000.).sin() * 0.1], &mut out);
            out[0]
        })
        .collect()
}
fn peak(values: &[f32]) -> f32 {
    values.iter().fold(0f32, |a, v| a.max(v.abs()))
}

#[test]
fn effects_transform_pcm_at_their_configured_frequencies() {
    assert!((peak(&samples(EffectKind::Gain, &[6.], 100.)) - 0.1995).abs() < 0.002);
    let mid = samples(EffectKind::Equalizer, &[1000., 2., 9.], 1000.);
    let low = samples(EffectKind::Equalizer, &[1000., 2., 9.], 100.);
    assert!(peak(&mid[1000..]) > 0.27 && peak(&low[1000..]) < 0.11);
    let mut eq = [0.; 12];
    eq[5] = 9.;
    eq[10] = 6.;
    assert!(peak(&samples(EffectKind::GraphicEqualizer, &eq, 1000.)[1000..]) > 0.27);
    assert!(peak(&samples(EffectKind::GraphicEqualizer, &eq, 31.)[5000..]) > 0.18);
    assert!(peak(&samples(EffectKind::Compressor, &[-40., 0., 10., 0., 0.5], 440.)[1500..]) < 0.02);
    let mut effect = Effect::new(EffectKind::Reverb);
    effect.values[0] = 1.;
    let mut unit = Unit::new(&effect, 48000, 1);
    let impulse: Vec<f32> = (0..8000)
        .map(|i| {
            let mut f = [f32::from(i == 0)];
            unit.apply(&mut f);
            f[0]
        })
        .collect();
    assert_eq!(peak(&impulse[..1400]), 0.);
    assert!(peak(&impulse[1400..]) > 0.005);
}

#[test]
fn levels_preserve_phase_and_lfe_never_downmixes_into_front_speakers() {
    let mut level = Level::new(2, 48000);
    let mut frame = [0.; 2];
    for _ in 0..48000 {
        frame = [0.9, -0.9];
        level.apply(&mut frame);
    }
    assert!(frame[0] < 0.15 && frame[0] > 0.10);
    assert_eq!(frame[0], -frame[1]);
    let mut route = Matrix::new(6, 2, false, 48000);
    let mut out = [0.; 2];
    route.apply(&[0., 0., 0., 1., 0., 0.], &mut out);
    assert_eq!(out, [0.; 2]);
    let mut surround = Matrix::new(2, 6, true, 48000);
    let mut out = [0.; 6];
    let mut rear = 0f32;
    for i in 0..3000 {
        surround.apply(&[f32::from(i == 0), 0.], &mut out);
        rear = rear.max(out[4].abs());
    }
    assert!(rear > 0.1);
    assert_eq!(
        target(
            Routing {
                layout: Layout::Surround71,
                expand_stereo: true
            },
            2,
            2
        ),
        2
    );
}

#[test]
fn pause_ramps_preserve_position_and_reverse_without_a_gain_jump() {
    let (mut producer, input, clock) = PcmSource::pipe(1, 1000, 0);
    let mut block = Block::empty();
    block.len = 200;
    block.data.fill(1.);
    producer.try_send(block).unwrap();
    let (mut stream, control) = RenderSource::new(input, 2, &Processing::default(), false);
    let mut frame = || {
        let left = stream.next().unwrap();
        assert_eq!(stream.next(), Some(left));
        left
    };
    let close = |actual: f32, expected: f32| assert!((actual - expected).abs() < 0.0001);
    close(frame(), 1.);
    control.pause.pause(10);
    for i in 1..=5 {
        close(frame(), 1. - i as f32 / 10.);
    }
    control.pause.resume(5, false);
    for i in 1..=5 {
        close(frame(), 0.5 + i as f32 / 10.);
    }
    control.pause.pause(3);
    close(frame(), 2. / 3.);
    close(frame(), 1. / 3.);
    close(frame(), 0.);
    let position = clock.position_ms();
    for _ in 0..20 {
        close(frame(), 0.);
    }
    assert_eq!(clock.position_ms(), position);
    control.pause.resume(4, true);
    for i in 1..=4 {
        close(frame(), i as f32 / 4.);
    }
    control.pause.pause(0);
    close(frame(), 0.);
    control.pause.resume(0, true);
    close(frame(), 1.);
}

#[test]
fn processing_updates_during_playback_and_seek_fade_obeys_requested_length() {
    let (mut producer, input, clock) = PcmSource::pipe(1, 1000, 0);
    clock
        .fades
        .incoming_ms
        .store(100, std::sync::atomic::Ordering::Relaxed);
    let mut block = Block::empty();
    block.len = 600;
    block.data.fill(0.1);
    producer.try_send(block).unwrap();
    drop(producer);
    let (mut stream, control) = RenderSource::new(input, 2, &Processing::default(), false);
    let mut take = |count| stream.by_ref().take(count).collect::<Vec<_>>();
    let fade = take(200);
    assert_eq!(&fade[..2], &[0., 0.]);
    assert_eq!(&fade[198..], &[0.1, 0.1]);
    let gain = Effect {
        enabled: true,
        kind: EffectKind::Gain,
        values: vec![6.],
    };
    let settings = Processing {
        dsp: LocalDsp {
            enabled: true,
            chain: vec![gain],
            ..Default::default()
        },
        ..Default::default()
    };
    control.update(&settings);
    let loud = take(180);
    assert!((loud[179] - 0.1995).abs() < 0.001);
    control.update(&Processing::default());
    let quiet = take(180);
    assert_eq!(quiet[179], 0.1);
    assert_eq!(clock.position_ms(), 280);
}

#[test]
fn dsp_refills_in_selected_frame_counts_without_skipping_audio_or_advancing_the_clock() {
    let (mut writer, input, clock) = PcmSource::pipe(2, 1000, 0);
    // 让解码块边界落在 DSP 块中间，最后一块不足请求的帧数。
    let mut sample = 0;
    for frames in [100, 500, 300] {
        let mut block = Block::empty();
        block.len = frames * 2;
        for value in &mut block.data[..block.len] {
            *value = sample as f32 / 2048.;
            sample += 1;
        }
        writer.try_send(block).unwrap();
    }
    drop(writer);
    let mut settings = Processing::default();
    settings.dsp.buffer_frames = 128;
    let (mut stream, control) = RenderSource::new(input, 2, &settings, false);
    for i in 0..1800 {
        assert_eq!(stream.next(), Some(i as f32 / 2048.));
        if i == 0 {
            assert_eq!(stream.block.len, 128);
            assert_eq!(clock.position_ms(), 1);
            settings.dsp.buffer_frames = 512;
            control.update(&settings);
        }
        if i == 256 {
            assert_eq!(stream.block.len, 512);
            assert_eq!(clock.position_ms(), 129);
        }
    }
    assert_eq!(stream.block.len, 260);
    assert_eq!(stream.next(), None);
    assert_eq!(clock.position_ms(), 900);
}

#[test]
fn device_change_reroutes_unplayed_frames_and_retains_position_while_paused() {
    let (mut writer, input, clock) = PcmSource::pipe(2, 1000, 1200);
    let mut block = Block::empty();
    block.len = 80;
    for (i, frame) in block.data[..block.len].chunks_exact_mut(2).enumerate() {
        frame[0] = i as f32 / 64.;
        frame[1] = -frame[0];
    }
    writer.try_send(block).unwrap();
    drop(writer);
    let settings = Processing::default();
    let (mut stream, _) = RenderSource::new(input, 1, &settings, false);
    assert_eq!(stream.next(), Some(0.));
    let control = stream.change_device(2, &settings, true);
    assert_eq!(stream.channels(), 2);
    assert!(stream.by_ref().take(8).all(|v| v == 0.));
    assert_eq!(clock.position_ms(), 1201);
    control.pause.resume(0, true);
    for i in 1..40 {
        let sample = i as f32 / 64.;
        assert_eq!(stream.next(), Some(sample));
        assert_eq!(stream.next(), Some(-sample));
    }
    assert_eq!(stream.next(), None);
    assert_eq!(clock.position_ms(), 1240);
}

#[test]
fn invalid_buffer_sizes_fall_back_to_default() {
    for &frames in &DSP_BUFFER_FRAMES {
        let mut dsp = LocalDsp {
            buffer_frames: frames,
            ..Default::default()
        };
        dsp.normalize();
        assert_eq!(dsp.buffer_frames, frames);
    }
    let mut dsp = LocalDsp {
        buffer_frames: usize::MAX,
        ..Default::default()
    };
    dsp.normalize();
    assert_eq!(dsp.buffer_frames, LocalDsp::default().buffer_frames);
}

#[test]
fn resizing_keeps_reverb_history() {
    let mut dsp = LocalDsp::default();
    let mut reverb = Effect::new(EffectKind::Reverb);
    reverb.values[0] = 1.;
    dsp.chain.push(reverb);
    let mut settings = Processing {
        dsp,
        ..Default::default()
    };
    let impulse = || {
        let (mut writer, input, _) = PcmSource::pipe(1, 48000, 0);
        let mut block = Block::empty();
        block.len = block.data.len();
        block.data[0] = 1.;
        writer.try_send(block).unwrap();
        input
    };
    let (reference, _) = RenderSource::new(impulse(), 1, &settings, false);
    let (mut resized, control) = RenderSource::new(impulse(), 1, &settings, false);
    let mut output: Vec<_> = resized.by_ref().take(100).collect();
    settings.dsp.buffer_frames = 1024;
    control.update(&settings);
    output.extend(resized);
    assert!(peak(&output[1400..]) > 0.005);
    assert_eq!(output, reference.collect::<Vec<_>>());
}
