use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub const FADE_MS: u64 = 300;
pub const MAX_FADE_MS: u64 = 2000;

// 生产者发布可用样本数；设备线程按音频帧计算增益，所有声道同步变化。
#[derive(Default)]
pub struct FadeControl {
    pub incoming_ms: AtomicU64,
    pub written: AtomicU64,
    pub finished: AtomicBool,
    pub track_tail_ms: AtomicU64,
    pub retiring_ms: AtomicU64,
}

pub struct Envelope {
    pub control: Arc<FadeControl>,
    rate: u32,
    natural_length: Option<u64>,
    manual: Option<(u64, u64, f32)>,
}

impl Envelope {
    pub fn new(rate: u32, control: Arc<FadeControl>) -> Self {
        Self {
            control,
            rate,
            natural_length: None,
            manual: None,
        }
    }

    pub fn gain(&mut self, frame: u64, channels: u16) -> Option<f32> {
        let frames = |ms: u64| (u64::from(self.rate) * ms / 1000).max(1);
        let fade_in = frames(self.control.incoming_ms.load(Ordering::Relaxed));
        let mut gain = 1.0 - remaining_gain(fade_in.saturating_sub(frame), fade_in);
        let frames_left = || {
            (self.control.written.load(Ordering::Acquire) / u64::from(channels))
                .saturating_sub(frame)
        };
        let tail_ms = self.control.track_tail_ms.load(Ordering::Relaxed);
        if tail_ms != 0 && self.control.finished.load(Ordering::Acquire) {
            // 解码到文件末尾才使用真实样本数，时长估算误差不会提前静音。
            let left = frames_left();
            let length = *self.natural_length.get_or_insert(left.min(frames(tail_ms)));
            gain = gain.min(remaining_gain(left, length));
        }
        let retiring_ms = self.control.retiring_ms.load(Ordering::Acquire);
        if retiring_ms != 0 {
            let (start, length, initial) = *self
                .manual
                .get_or_insert_with(|| (frame, frames_left().min(frames(retiring_ms)), gain));
            let left = length.saturating_sub(frame - start);
            if left == 0 {
                return None;
            }
            gain = gain.min(initial * remaining_gain(left, length));
        }
        Some(gain)
    }
}

fn remaining_gain(left: u64, length: u64) -> f32 {
    if left <= 1 {
        0.0
    } else if left >= length {
        1.0
    } else {
        (left - 1) as f32 / (length - 1) as f32
    }
}
