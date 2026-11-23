use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub const FADE_MS: u64 = 300;
pub const MAX_FADE_MS: u64 = 2000;

// 生产者发布可用样本数；设备线程按音频帧计算增益，所有声道同步变化。
pub struct FadeControl {
    pub duration_ms: AtomicU64,
    pub written: AtomicU64,
    pub finished: AtomicBool,
    pub track_tail: AtomicBool,
    pub retiring: AtomicBool,
}
impl Default for FadeControl {
    fn default() -> Self {
        Self {
            duration_ms: AtomicU64::new(FADE_MS),
            written: 0.into(),
            finished: false.into(),
            track_tail: false.into(),
            retiring: false.into(),
        }
    }
}

pub struct Envelope {
    pub control: Arc<FadeControl>,
    pub fade_in: bool,
    rate: u32,
    natural_length: Option<u64>,
    manual: Option<(u64, u64, f32)>,
}

impl Envelope {
    pub fn new(rate: u32, control: Arc<FadeControl>) -> Self {
        Self {
            control,
            fade_in: false,
            rate,
            natural_length: None,
            manual: None,
        }
    }

    pub fn gain(&mut self, frame: u64, channels: u16) -> Option<f32> {
        let frames =
            (u64::from(self.rate) * self.control.duration_ms.load(Ordering::Relaxed) / 1000).max(1);
        let mut gain = if self.fade_in {
            1.0 - remaining_gain(frames.saturating_sub(frame), frames)
        } else {
            1.0
        };
        let frames_left = || {
            (self.control.written.load(Ordering::Acquire) / u64::from(channels))
                .saturating_sub(frame)
        };
        if self.control.track_tail.load(Ordering::Relaxed)
            && self.control.finished.load(Ordering::Acquire)
        {
            // 解码到文件末尾才使用真实样本数，时长估算误差不会提前静音。
            let left = frames_left();
            let length = *self.natural_length.get_or_insert(left.min(frames));
            gain = gain.min(remaining_gain(left, length));
        }
        if self.control.retiring.load(Ordering::Acquire) {
            let (start, length, initial) = *self
                .manual
                .get_or_insert_with(|| (frame, frames_left().min(frames), gain));
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
