// 取最响声道的 RMS 及全部声道的峰值；单一增益保持立体声、环绕声的声像。
pub struct Level {
    squares: Vec<f64>,
    peak: f32,
    frames: u32,
    interval: u32,
    warmup: u32,
    db: f32,
    target: f32,
    gain: f32,
    fast: f32,
    slow: f32,
}
impl Level {
    pub fn new(ch: usize, rate: u32) -> Self {
        Self {
            squares: vec![0.; ch],
            peak: 0.,
            frames: 0,
            interval: (rate / 10).max(1),
            warmup: 0,
            db: 0.,
            target: 1.,
            gain: 1.,
            fast: (-1. / (0.02 * rate as f32)).exp(),
            slow: (-1. / (0.08 * rate as f32)).exp(),
        }
    }
    pub fn apply(&mut self, frame: &mut [f32]) {
        for (sum, v) in self.squares.iter_mut().zip(frame.iter()) {
            *sum += f64::from(*v) * f64::from(*v);
            self.peak = self.peak.max(v.abs());
        }
        self.frames += 1;
        if self.frames >= self.interval {
            let rms = (self.squares.iter().copied().fold(0., f64::max) / f64::from(self.frames))
                .sqrt() as f32;
            if rms < 0.00316228 {
                self.warmup = 0;
            } else {
                self.warmup = self.warmup.saturating_add(1);
                let peak_limit = 20. * (0.95 / self.peak.max(1e-10)).log10();
                let wanted = (-18. - 20. * rms.log10()).min(peak_limit).clamp(-24., 6.);
                if wanted <= self.db || self.warmup >= 4 {
                    let seconds = if wanted < self.db { 0.04_f32 } else { 2.0 };
                    self.db = (self.db + (wanted - self.db) * (1. - (-0.1 / seconds).exp()))
                        .min(peak_limit)
                        .max(-24.);
                    self.target = 10f32.powf(self.db / 20.);
                }
            }
            self.peak = 0.;
            self.squares.fill(0.);
            self.frames = 0;
        }
        let k = if self.target < self.gain {
            self.fast
        } else {
            self.slow
        };
        self.gain = self.target + k * (self.gain - self.target);
        for v in frame {
            *v *= self.gain;
        }
    }
}
