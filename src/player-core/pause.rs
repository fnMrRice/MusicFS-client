use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

const PENDING_PAUSE: u32 = 1 << 30;
const FROM_SILENCE: u32 = 1 << 29;
const NO_CHANGE: u32 = u32::MAX;

pub struct PauseControl {
    request: AtomicU32,
    pub silent: AtomicBool,
}

impl PauseControl {
    pub fn pause(&self, ms: u64) {
        self.request
            .store(PENDING_PAUSE | ms as u32, Ordering::Release);
    }

    pub fn resume(&self, ms: u64, from_silence: bool) {
        let start = if from_silence { FROM_SILENCE } else { 0 };
        self.request.store(start | ms as u32, Ordering::Release);
    }
}

// 在设备线程按输出帧改变增益。静音后冻结媒体位置，等播放器挂起 Sink。
pub struct PauseEnvelope {
    pub control: Arc<PauseControl>,
    rate: u32,
    gain: f32,
    target: f32,
    remaining: u64,
}

impl PauseEnvelope {
    pub fn new(rate: u32, paused: bool) -> Self {
        let gain = if paused { 0.0 } else { 1.0 };
        Self {
            control: Arc::new(PauseControl {
                request: NO_CHANGE.into(),
                silent: paused.into(),
            }),
            rate,
            gain,
            target: gain,
            remaining: 0,
        }
    }

    pub fn holding(&mut self) -> bool {
        if self.control.request.load(Ordering::Relaxed) != NO_CHANGE {
            let command = self.control.request.swap(NO_CHANGE, Ordering::AcqRel);
            let ms = command & !(PENDING_PAUSE | FROM_SILENCE);
            self.target = if command & PENDING_PAUSE != 0 {
                0.0
            } else {
                1.0
            };
            if command & FROM_SILENCE != 0 {
                self.gain = 0.0;
            }
            self.remaining = u64::from(self.rate) * u64::from(ms) / 1000;
            if self.remaining == 0 {
                self.gain = self.target;
            }
            self.control
                .silent
                .store(self.target == 0.0 && self.gain == 0.0, Ordering::Release);
        }
        self.target == 0.0 && self.gain == 0.0
    }

    pub fn next_gain(&mut self, has_media: bool) -> f32 {
        if self.remaining != 0 && (self.target == 0.0 || has_media) {
            self.gain += (self.target - self.gain) / self.remaining as f32;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.gain = self.target;
                self.control
                    .silent
                    .store(self.target == 0.0, Ordering::Release);
            }
        }
        self.gain
    }
}
