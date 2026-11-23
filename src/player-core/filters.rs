use crate::audio::{Effect, EffectKind, FREQUENCIES};
use std::f32::consts::{FRAC_1_SQRT_2, PI, SQRT_2};

pub enum FilterType {
    Peak,
    Bass,
    Treble,
    LowPass,
    HighPass,
}

pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    history: Vec<[f32; 2]>,
}
impl Biquad {
    pub fn new(kind: FilterType, hz: f32, q: f32, db: f32, rate: u32, channels: usize) -> Self {
        let omega = 2. * PI * (hz / rate as f32).clamp(0.000001, 0.495);
        let (s, c) = omega.sin_cos();
        let gain = 10f32.powf(db / 40.);
        let alpha = s / (2. * q);
        let beta = s * (2. * gain).sqrt();
        let g1 = gain + 1.;
        let g2 = gain - 1.;
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterType::Peak => (
                1. + alpha * gain,
                -2. * c,
                1. - alpha * gain,
                1. + alpha / gain,
                -2. * c,
                1. - alpha / gain,
            ),
            FilterType::LowPass => (
                (1. - c) / 2.,
                1. - c,
                (1. - c) / 2.,
                1. + alpha,
                -2. * c,
                1. - alpha,
            ),
            FilterType::HighPass => (
                (1. + c) / 2.,
                -(1. + c),
                (1. + c) / 2.,
                1. + alpha,
                -2. * c,
                1. - alpha,
            ),
            FilterType::Bass => (
                gain * (g1 - g2 * c + beta),
                2. * gain * (g2 - g1 * c),
                gain * (g1 - g2 * c - beta),
                g1 + g2 * c + beta,
                -2. * (g2 + g1 * c),
                g1 + g2 * c - beta,
            ),
            FilterType::Treble => (
                gain * (g1 + g2 * c + beta),
                -2. * gain * (g2 + g1 * c),
                gain * (g1 + g2 * c - beta),
                g1 - g2 * c + beta,
                2. * (g2 - g1 * c),
                g1 - g2 * c - beta,
            ),
        };
        Self {
            b: [b0 / a0, b1 / a0, b2 / a0],
            a: [a1 / a0, a2 / a0],
            history: vec![[0.; 2]; channels],
        }
    }
    pub fn sample(&mut self, ch: usize, x: f32) -> f32 {
        let state = &mut self.history[ch];
        let out = self.b[0] * x + state[0];
        state[0] = self.b[1] * x - self.a[0] * out + state[1];
        state[1] = self.b[2] * x - self.a[1] * out;
        out
    }
}

pub enum Unit {
    Gain(f32),
    Eq(Vec<Biquad>),
    Compressor(Compressor),
    Reverb(f32, Vec<Comb>),
}
impl Unit {
    pub fn new(effect: &Effect, rate: u32, channels: usize) -> Self {
        let p = &effect.values;
        match effect.kind {
            EffectKind::Gain => Self::Gain(10f32.powf(p[0] / 20.)),
            EffectKind::Equalizer => Self::Eq(vec![Biquad::new(
                FilterType::Peak,
                p[0],
                p[1],
                p[2],
                rate,
                channels,
            )]),
            EffectKind::GraphicEqualizer => {
                let mut chain = Vec::with_capacity(12);
                for (i, hz) in FREQUENCIES.iter().enumerate() {
                    if p[i] != 0. {
                        chain.push(Biquad::new(
                            FilterType::Peak,
                            *hz,
                            SQRT_2,
                            p[i],
                            rate,
                            channels,
                        ));
                    }
                }
                for (i, kind, hz) in [
                    (10, FilterType::Bass, 120.),
                    (11, FilterType::Treble, 6000.),
                ] {
                    if p[i] != 0. {
                        chain.push(Biquad::new(kind, hz, FRAC_1_SQRT_2, p[i], rate, channels));
                    }
                }
                Self::Eq(chain)
            }
            EffectKind::Compressor => Self::Compressor(Compressor::new(p, rate, channels)),
            EffectKind::Reverb => Self::Reverb(
                p[0],
                [0.0297, 0.0371, 0.0411, 0.0437]
                    .into_iter()
                    .map(|s| Comb::new(s, p[1], p[2], rate, channels))
                    .collect(),
            ),
        }
    }
    pub fn apply(&mut self, frame: &mut [f32]) {
        match self {
            Self::Gain(gain) => frame.iter_mut().for_each(|v| *v *= *gain),
            Self::Eq(chain) => {
                for filter in chain {
                    for (ch, v) in frame.iter_mut().enumerate() {
                        *v = filter.sample(ch, *v);
                    }
                }
            }
            Self::Compressor(comp) => comp.apply(frame),
            Self::Reverb(mix, combs) => {
                for (ch, v) in frame.iter_mut().enumerate() {
                    let wet: f32 = combs.iter_mut().map(|c| c.sample(ch, *v)).sum();
                    *v = *v * (1. - *mix) + wet * 0.25 * *mix;
                }
                for comb in combs {
                    comb.advance();
                }
            }
        }
    }
}

pub struct Compressor {
    threshold: f32,
    knee: f32,
    slope: f32,
    attack: f32,
    release: f32,
    gains: Vec<f32>,
}
impl Compressor {
    fn new(p: &[f32], rate: u32, channels: usize) -> Self {
        let coeff = |t: f32| {
            if t <= 0. {
                0.
            } else {
                (-1. / (t * rate as f32)).exp()
            }
        };
        Self {
            threshold: p[0],
            knee: p[1],
            slope: 1. / p[2] - 1.,
            attack: coeff(p[3]),
            release: coeff(p[4]),
            gains: vec![1.; if channels <= 2 { 1 } else { channels }],
        }
    }
    fn gain(&mut self, ch: usize, peak: f32) -> f32 {
        let over = 20. * peak.max(1e-10).log10() - self.threshold;
        let db = if over < -self.knee / 2. {
            0.
        } else if self.knee > 0. && over <= self.knee / 2. {
            self.slope * (over + self.knee / 2.).powi(2) / (2. * self.knee)
        } else {
            self.slope * over
        };
        let want = 10f32.powf(db / 20.);
        let g = &mut self.gains[ch];
        let coeff = if want < *g { self.attack } else { self.release };
        *g = want + coeff * (*g - want);
        *g
    }
    fn apply(&mut self, frame: &mut [f32]) {
        if self.gains.len() == 1 {
            let gain = self.gain(0, frame.iter().fold(0f32, |a, v| a.max(v.abs())));
            frame.iter_mut().for_each(|v| *v *= gain);
        } else {
            for (ch, v) in frame.iter_mut().enumerate() {
                *v *= self.gain(ch, v.abs());
            }
        }
    }
}

pub struct Comb {
    buffer: Vec<f32>,
    cursor: usize,
    channels: usize,
    feedback: f32,
    low: Biquad,
}
impl Comb {
    fn new(seconds: f32, decay: f32, tone: f32, rate: u32, channels: usize) -> Self {
        Self {
            buffer: vec![0.; ((seconds * rate as f32).round() as usize).max(1) * channels],
            cursor: 0,
            channels,
            feedback: 0.001f32.powf(seconds / decay),
            low: Biquad::new(FilterType::LowPass, tone, FRAC_1_SQRT_2, 0., rate, channels),
        }
    }
    fn sample(&mut self, ch: usize, v: f32) -> f32 {
        let i = self.cursor + ch;
        let delayed = self.low.sample(ch, self.buffer[i]);
        self.buffer[i] = v * (1. - self.feedback) + delayed * self.feedback;
        delayed
    }
    fn advance(&mut self) {
        self.cursor = (self.cursor + self.channels) % self.buffer.len();
    }
}
