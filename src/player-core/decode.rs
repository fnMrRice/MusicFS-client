use crate::pcm::{Block, Clock, PcmSource};
use crate::{AudioReader, Media, PlaybackFormat, Track};
use rodio::{Decoder, Source};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct Lifecycle {
    pub started: Option<Box<dyn FnOnce() + Send>>,
    close: Option<Box<dyn FnOnce() + Send>>,
    pub source_id: String,
    pub format: PlaybackFormat,
    pub seek_range_ms: Option<(u64, u64)>,
}

impl Drop for Lifecycle {
    fn drop(&mut self) {
        if let Some(close) = self.close.take() {
            close();
        }
    }
}

pub enum Event {
    Opened(Lifecycle),
    Ready {
        revision: u64,
        source: Box<PcmSource>,
        clock: Clock,
        duration_ms: Option<u64>,
        seekable: bool,
    },
    Failed(String),
}

type SeekRequest = (u64, u64);
struct Job {
    generation: u64,
    track: Track,
    seeks: Receiver<SeekRequest>,
}

pub struct Worker {
    jobs: Sender<Job>,
    pub events: Receiver<(u64, Event)>,
    generation: Arc<AtomicU64>,
}

impl Worker {
    pub fn new(
        loader: impl Fn(&Track) -> Result<Media, String> + Send + 'static,
    ) -> Result<Self, String> {
        let (jobs, incoming) = channel::<Job>();
        let (send, events) = channel();
        let generation = Arc::new(AtomicU64::new(0));
        let active = generation.clone();
        thread::Builder::new()
            .name("player-decode".into())
            .spawn(move || {
                while let Ok(job) = incoming.recv() {
                    if job.generation != active.load(Ordering::Acquire) {
                        continue;
                    }
                    let result =
                        loader(&job.track).and_then(|media| decode(&job, &active, &send, media));
                    if let Err(error) = result {
                        let _ = send.send((job.generation, Event::Failed(error)));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            jobs,
            events,
            generation,
        })
    }

    pub fn cancel(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn load(&self, generation: u64, track: Track) -> Result<Sender<SeekRequest>, String> {
        let (seeks, receiver) = channel();
        self.jobs
            .send(Job {
                generation,
                track,
                seeks: receiver,
            })
            .map_err(|_| "Decoder is unavailable.")?;
        Ok(seeks)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}

// 解码库的迭代接口以 None 表示读尽；保存底层读取错误，避免把断线当作正常曲终。
struct ObservedReader {
    input: Box<dyn AudioReader>,
    error: Arc<Mutex<Option<String>>>,
}

impl Read for ObservedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let result = self.input.read(buf);
        if let Err(error) = &result {
            if error.kind() != io::ErrorKind::Interrupted {
                *self.error.lock().unwrap() = Some(error.to_string());
            }
        }
        result
    }
}

impl Seek for ObservedReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.input.seek(pos)
    }
}

fn decode(
    job: &Job,
    active: &AtomicU64,
    events: &Sender<(u64, Event)>,
    media: Media,
) -> Result<(), String> {
    let lifecycle = Lifecycle {
        started: Some(media.on_started),
        close: Some(media.on_close),
        source_id: media.source_track_id,
        format: media.format,
        seek_range_ms: media.seek_range_ms,
    };
    let current = || job.generation == active.load(Ordering::Acquire);
    if !current() {
        return Ok(());
    }
    let emit = |event| {
        events
            .send((job.generation, event))
            .map_err(|_| "Player stopped.".to_string())
    };
    emit(Event::Opened(lifecycle))?;
    let error = Arc::new(Mutex::new(None));
    let input = BufReader::new(ObservedReader {
        input: media.reader,
        error: error.clone(),
    });
    let mut builder = Decoder::builder().with_data(input);
    if let Some(bytes) = media.byte_length {
        builder = builder.with_byte_len(bytes);
    }
    let seekable = media.seekable && media.byte_length.is_some();
    let mut decoder = builder
        .with_seekable(seekable)
        .build()
        .map_err(|e| format!("Audio decoding failed: {e}"))?;
    let duration_ms = decoder.total_duration().map(|d| d.as_millis() as u64);
    let mut seek = (0, 0);
    while current() {
        let (revision, origin_ms) = seek;
        if revision != 0 {
            decoder
                .try_seek(Duration::from_millis(origin_ms))
                .map_err(|e| format!("Audio seek failed: {e}"))?;
        }
        let channels = decoder.channels();
        let rate = decoder.sample_rate();
        if channels == 0 || rate == 0 {
            return Err("Unsupported audio format.".into());
        }
        if usize::from(channels) > crate::processing::MAX_CHANNELS {
            return Err("Audio output supports up to 32 channels.".into());
        }
        let frame_samples = usize::from(channels);
        let block_samples = Block::aligned_samples(channels);
        let (producer, source, clock) = PcmSource::pipe(channels, rate, origin_ms);
        let mut ready = Some(Event::Ready {
            revision,
            source: Box::new(source),
            clock,
            duration_ms,
            seekable,
        });
        let mut producer = Some(producer);
        let mut pending = None;
        loop {
            if !current() {
                return Ok(());
            }
            if let Some(next) = job.seeks.try_iter().last() {
                seek = next;
                break;
            }
            let Some(tx) = &mut producer else {
                match job.seeks.recv_timeout(Duration::from_millis(50)) {
                    Ok(next) => {
                        seek = job.seeks.try_iter().last().unwrap_or(next);
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                }
            };
            let block = pending.take().unwrap_or_else(|| {
                let mut block = Block::empty();
                for sample in &mut block.data[..block_samples] {
                    let Some(value) = decoder.next() else { break };
                    *sample = value;
                    block.len += 1;
                }
                block.len -= block.len % frame_samples;
                block
            });
            if block.len == 0 {
                if let Some(message) = error.lock().unwrap().take() {
                    return Err(message);
                }
                if ready.is_some() && revision == 0 {
                    return Err("No audio samples were decoded.".into());
                }
                if let Some(ready) = ready.take() {
                    emit(ready)?;
                }
                drop(producer.take());
                continue;
            }
            match tx.try_send(block) {
                Ok(()) => {
                    if let Some(ready) = ready.take() {
                        emit(ready)?;
                    }
                }
                Err(TrySendError::Full(block)) => {
                    pending = Some(block);
                    thread::sleep(Duration::from_millis(4));
                }
                Err(TrySendError::Disconnected(_)) => {
                    drop(producer.take());
                }
            }
        }
    }
    Ok(())
}
