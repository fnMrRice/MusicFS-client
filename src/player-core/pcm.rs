use rodio::Source;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

// 解码线程和设备线程间只传输定长 PCM 块，队列满时由解码线程等待。
pub const BLOCK_SAMPLES: usize = 2048;
pub struct Block {
    pub data: [f32; BLOCK_SAMPLES],
    pub len: usize,
}

impl Block {
    pub fn empty() -> Self {
        Self {
            data: [0.0; BLOCK_SAMPLES],
            len: 0,
        }
    }
}

#[derive(Clone)]
pub struct Clock {
    samples: Arc<AtomicU64>,
    samples_per_second: u64,
    origin_ms: u64,
}

impl Clock {
    pub fn started(&self) -> bool {
        self.samples.load(Ordering::Relaxed) > 0
    }
    pub fn position_ms(&self) -> u64 {
        let samples = self.samples.load(Ordering::Relaxed);
        self.origin_ms + samples.saturating_mul(1000) / self.samples_per_second
    }
}

pub struct PcmSource {
    incoming: Receiver<Block>,
    current: Block,
    cursor: usize,
    silence: u16,
    channels: u16,
    rate: u32,
    clock: Clock,
    played: u64,
}

impl PcmSource {
    pub fn pipe(channels: u16, rate: u32, origin_ms: u64) -> (SyncSender<Block>, Self, Clock) {
        let (producer, incoming) = sync_channel(16);
        let clock = Clock {
            samples: Arc::new(AtomicU64::new(0)),
            origin_ms,
            samples_per_second: u64::from(channels) * u64::from(rate),
        };
        let source = Self {
            incoming,
            current: Block::empty(),
            cursor: 0,
            silence: 0,
            channels,
            rate,
            clock: clock.clone(),
            played: 0,
        };
        (producer, source, clock)
    }
}

impl Iterator for PcmSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.silence != 0 {
            self.silence -= 1;
            return Some(0.0);
        }
        if self.cursor == self.current.len {
            match self.incoming.try_recv() {
                Ok(block) => {
                    self.current = block;
                    self.cursor = 0;
                }
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => {
                    // 欠载以整帧静音填充；只用真实媒体样本推进时钟。
                    self.silence = self.channels - 1;
                    return Some(0.0);
                }
            }
        }
        let sample = self.current.data[self.cursor];
        self.cursor += 1;
        self.played += 1;
        self.clock.samples.store(self.played, Ordering::Relaxed);
        Some(sample)
    }
}

impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
