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
fn processing_updates_during_playback_and_seek_fade_obeys_requested_length() {
    let (mut producer, mut input, clock) = PcmSource::pipe(1, 1000, 0);
    input.envelope.fade_in = true;
    clock
        .fades
        .duration_ms
        .store(100, std::sync::atomic::Ordering::Relaxed);
    let mut block = Block::empty();
    block.len = 600;
    block.data.fill(0.1);
    producer.try_send(block).unwrap();
    drop(producer);
    let (mut stream, control) = RenderSource::new(input, 2, &Processing::default());
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
