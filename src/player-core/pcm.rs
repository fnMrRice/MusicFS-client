use crate::fade::{Envelope, FadeControl, MAX_FADE_MS};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;

// 解码线程和设备线程间只传输定长 PCM 块，队列满时由解码线程等待。
pub const BLOCK_SAMPLES: usize = 2048;
pub struct Block {
    pub data: [f32; BLOCK_SAMPLES],
    pub len: usize,
}

impl Block {
    pub fn aligned_samples(channels: u16) -> usize {
        let channels = usize::from(channels);
        BLOCK_SAMPLES / channels * channels
    }

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
    channels: u16,
    rate: u32,
    clock: Clock,
    played: u64,
    envelope: Envelope,
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
        let fade_blocks = (samples_per_second * MAX_FADE_MS)
            .div_ceil(1000 * Block::aligned_samples(channels) as u64);
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
            channels,
            rate,
            clock: clock.clone(),
            played: 0,
            envelope: Envelope::new(rate, fades),
            gain: 1.0,
        };
        (producer, source, clock)
    }

    // 解码块和目标缓冲均按完整声道帧对齐；先处理已有数据，不等待下一块。
    pub fn read_frames(&mut self, out: &mut [f32]) -> Result<usize, TryRecvError> {
        let mut len = 0;
        while len < out.len() {
            if self.cursor == self.current.len {
                match self.incoming.try_recv() {
                    Ok(block) => {
                        self.current = block;
                        self.cursor = 0;
                    }
                    Err(_) if len > 0 => break,
                    Err(TryRecvError::Empty)
                        if self.clock.fades.retiring_ms.load(Ordering::Acquire) != 0 =>
                    {
                        return Err(TryRecvError::Disconnected);
                    }
                    Err(e) => return Err(e),
                }
            }
            let count = (out.len() - len).min(self.current.len - self.cursor);
            out[len..len + count]
                .copy_from_slice(&self.current.data[self.cursor..self.cursor + count]);
            self.cursor += count;
            len += count;
        }
        Ok(len / usize::from(self.channels))
    }

    // 淡变和时钟跟随实际输出的媒体帧，预读以及欠载静音都不提前消耗进度。
    pub fn emit_frame(&mut self, has_media: bool) -> Option<f32> {
        if has_media {
            self.gain = self
                .envelope
                .gain(self.played / u64::from(self.channels), self.channels)?;
            self.played += u64::from(self.channels);
            self.clock.samples.store(self.played, Ordering::Relaxed);
        }
        Some(self.gain)
    }
    pub fn channels(&self) -> u16 {
        self.channels
    }
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }
}
