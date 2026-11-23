use crate::fade::{Envelope, FadeControl, MAX_FADE_MS};
use rodio::Source;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError, TrySendError};
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
    pub fades: Arc<FadeControl>,
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
    pub envelope: Envelope,
    gain: f32,
}

pub struct Producer {
    sender: SyncSender<Block>,
    fades: Arc<FadeControl>,
    written: u64,
}

impl Producer {
    pub fn try_send(&mut self, block: Block) -> Result<(), TrySendError<Block>> {
        let len = block.len as u64;
        self.sender.try_send(block)?;
        self.written += len;
        self.fades.written.store(self.written, Ordering::Release);
        Ok(())
    }
}

impl Drop for Producer {
    fn drop(&mut self) {
        self.fades.finished.store(true, Ordering::Release);
    }
}

impl PcmSource {
    pub fn pipe(channels: u16, rate: u32, origin_ms: u64) -> (Producer, Self, Clock) {
        let samples_per_second = u64::from(channels) * u64::from(rate);
        let fade_blocks = (samples_per_second * MAX_FADE_MS).div_ceil(1000 * BLOCK_SAMPLES as u64);
        let (sender, incoming) = sync_channel(fade_blocks.clamp(15, 4096) as usize + 1);
        let fades = Arc::new(FadeControl::default());
        let producer = Producer {
            sender,
            fades: fades.clone(),
            written: 0,
        };
        let clock = Clock {
            samples: Arc::new(AtomicU64::new(0)),
            origin_ms,
            samples_per_second,
            fades: fades.clone(),
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
            envelope: Envelope::new(rate, fades),
            gain: 1.0,
        };
        (producer, source, clock)
    }
}

impl PcmSource {
    pub fn gain(&self) -> f32 {
        self.gain
    }
    pub fn next_raw(&mut self) -> Option<f32> {
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
                    if self.clock.fades.retiring.load(Ordering::Acquire) {
                        return None;
                    }
                    // 欠载以整帧静音填充；只用真实媒体样本推进时钟。
                    self.silence = self.channels - 1;
                    return Some(0.0);
                }
            }
        }
        if self.played.is_multiple_of(u64::from(self.channels)) {
            self.gain = self
                .envelope
                .gain(self.played / u64::from(self.channels), self.channels)?;
        }
        let sample = self.current.data[self.cursor];
        self.cursor += 1;
        self.played += 1;
        self.clock.samples.store(self.played, Ordering::Relaxed);
        Some(sample)
    }
}

impl Iterator for PcmSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.next_raw().map(|v| v * self.gain)
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
