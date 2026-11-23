use crate::pcm::{Clock, PcmSource};
use crate::Status;
use rodio::{OutputStream, OutputStreamBuilder, Sink};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct Output {
    device: Option<OutputStream>,
    lost: Arc<AtomicBool>,
    sink: Option<Sink>,
    retiring: Option<Sink>,
    pub clock: Option<Clock>,
}

impl Output {
    pub fn attach(
        &mut self,
        source: PcmSource,
        clock: Clock,
        volume: f32,
        paused: bool,
    ) -> Result<(), String> {
        if self.device.is_none() {
            self.lost = Arc::new(AtomicBool::new(false));
            let lost = self.lost.clone();
            let builder = OutputStreamBuilder::from_default_device()
                .map_err(|e| format!("Audio output device is unavailable: {e}"))?;
            let mut device = builder
                .with_error_callback(move |_| lost.store(true, Ordering::Release))
                .open_stream_or_fallback()
                .map_err(|e| format!("Audio output failed: {e}"))?;
            device.log_on_drop(false);
            self.device = Some(device);
        }
        let sink = Sink::connect_new(self.device.as_ref().unwrap().mixer());
        sink.set_volume(volume);
        if paused || self.retiring.is_some() {
            sink.pause();
        }
        sink.append(source);
        self.clock = Some(clock);
        self.sink = Some(sink);
        Ok(())
    }

    pub fn detach(&mut self, fade_out: bool) {
        if !fade_out {
            self.retiring = None;
        }
        if let (Some(sink), Some(clock)) = (self.sink.take(), self.clock.take()) {
            if fade_out && !sink.is_paused() && !sink.empty() && clock.started() {
                clock.fades.retiring.store(true, Ordering::Release);
                self.retiring = Some(sink);
            }
        }
    }

    pub fn poll(&mut self, paused: bool) -> Result<(), String> {
        if self.lost.swap(false, Ordering::AcqRel) {
            return Err("Audio output device disconnected.".into());
        }
        if self.retiring.as_ref().is_some_and(Sink::empty) {
            self.retiring = None;
            if !paused {
                self.play();
            }
        }
        Ok(())
    }

    pub fn play(&self) {
        if self.retiring.is_none() {
            if let Some(sink) = &self.sink {
                sink.play();
            }
        }
    }

    pub fn pause(&mut self) {
        self.retiring = None;
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    pub fn set_volume(&self, volume: f32) {
        for sink in self.sink.iter().chain(&self.retiring) {
            sink.set_volume(volume);
        }
    }

    pub fn status(&self, paused: bool) -> Status {
        if paused {
            Status::Paused
        } else if self.sink.is_some() && self.retiring.is_none() {
            Status::Playing
        } else {
            Status::Loading
        }
    }

    pub fn ended(&self) -> bool {
        self.sink.as_ref().is_some_and(Sink::empty)
    }
}
