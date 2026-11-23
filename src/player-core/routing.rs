use crate::audio::Routing;
use crate::filters::{Biquad, FilterType};
use std::f32::consts::FRAC_1_SQRT_2;

#[derive(Clone, Copy, PartialEq)]
enum Speaker {
    M,
    L,
    R,
    C,
    Lfe,
    Bl,
    Br,
    Sl,
    Sr,
}
use Speaker::*;
fn speakers(n: usize) -> Option<&'static [Speaker]> {
    match n {
        1 => Some(&[M]),
        2 => Some(&[L, R]),
        6 => Some(&[L, R, C, Lfe, Bl, Br]),
        8 => Some(&[L, R, C, Lfe, Bl, Br, Sl, Sr]),
        _ => None,
    }
}
pub fn target(routing: Routing, source: u16, physical: u16) -> u16 {
    let requested = routing.layout.channels(source);
    if requested <= physical {
        requested
    } else {
        [8, 6, 2, 1]
            .into_iter()
            .find(|n| *n <= physical)
            .unwrap_or(1)
    }
}

pub struct Matrix {
    rows: Vec<Row>,
    direct: bool,
}
struct Row {
    weights: Vec<(usize, f32)>,
    filter: Option<Biquad>,
    delay: Vec<f32>,
    cursor: usize,
}
impl Matrix {
    pub fn new(source: usize, output: usize, expand: bool, rate: u32) -> Self {
        let rows = if source == output {
            vec![]
        } else {
            (0..output)
                .map(|i| Row::new(i, source, output, expand, rate))
                .collect()
        };
        Self {
            rows,
            direct: source == output,
        }
    }
    pub fn apply(&mut self, src: &[f32], dst: &mut [f32]) {
        if self.direct {
            dst.copy_from_slice(src);
            return;
        }
        for (row, sample) in self.rows.iter_mut().zip(dst) {
            let mut value = row.weights.iter().map(|(ch, w)| src[*ch] * w).sum();
            if let Some(filter) = &mut row.filter {
                value = filter.sample(0, value);
            }
            if !row.delay.is_empty() {
                std::mem::swap(&mut value, &mut row.delay[row.cursor]);
                row.cursor = (row.cursor + 1) % row.delay.len();
            }
            *sample = value;
        }
    }
}
impl Row {
    fn new(i: usize, src: usize, out: usize, expand: bool, rate: u32) -> Self {
        let mut row = Self {
            weights: vec![],
            filter: None,
            delay: vec![],
            cursor: 0,
        };
        let (Some(source), Some(output)) = (speakers(src), speakers(out)) else {
            if i < src {
                row.weights.push((i, 1.));
            }
            return row;
        };
        let speaker = output[i];
        for (ch, &input) in source.iter().enumerate() {
            let weight = if src == 2 && out >= 6 && expand {
                match speaker {
                    L | R => f32::from(speaker == input),
                    C => 0.5,
                    Lfe => 0.25,
                    Bl | Br | Sl | Sr => {
                        let left = matches!(speaker, Bl | Sl);
                        let level = if out == 8 && matches!(speaker, Bl | Br) {
                            0.35
                        } else {
                            0.5
                        };
                        if left == (input == L) {
                            level
                        } else {
                            -level
                        }
                    }
                    M => 0.,
                }
            } else if speaker == M {
                match input {
                    Lfe => 0.,
                    C | M => 1.,
                    _ => FRAC_1_SQRT_2,
                }
            } else if src == 1 {
                f32::from(speaker == C || (!output.contains(&C) && matches!(speaker, L | R)))
            } else if out == 6
                && src == 8
                && ((speaker == Bl && matches!(input, Bl | Sl))
                    || (speaker == Br && matches!(input, Br | Sr)))
            {
                FRAC_1_SQRT_2
            } else if input == speaker {
                1.
            } else if out == 2
                && (input == C
                    || (speaker == L && matches!(input, Bl | Sl))
                    || (speaker == R && matches!(input, Br | Sr)))
            {
                FRAC_1_SQRT_2
            } else {
                0.
            };
            if weight != 0. {
                row.weights.push((ch, weight));
            }
        }
        let sum: f32 = row.weights.iter().map(|(_, w)| w.abs()).sum();
        if sum > 1. {
            for (_, w) in &mut row.weights {
                *w /= sum;
            }
        }
        if src == 2 && out >= 6 && expand {
            if speaker == Lfe {
                row.filter = Some(Biquad::new(
                    FilterType::LowPass,
                    120.,
                    FRAC_1_SQRT_2,
                    0.,
                    rate,
                    1,
                ));
            } else if matches!(speaker, Bl | Br | Sl | Sr) {
                row.filter = Some(Biquad::new(
                    FilterType::HighPass,
                    180.,
                    FRAC_1_SQRT_2,
                    0.,
                    rate,
                    1,
                ));
                let seconds = if out == 8 && matches!(speaker, Bl | Br) {
                    0.027
                } else {
                    0.015
                };
                let seconds = seconds
                    + if matches!(speaker, Br | Sr) {
                        0.006
                    } else {
                        0.
                    };
                row.delay = vec![0.; (seconds * rate as f32).round() as usize];
            }
        }
        row
    }
}
