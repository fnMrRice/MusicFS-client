use crate::pcm::{Clock, PcmSource};
use crate::{
    audio::{OutputStatus, Processing},
    processing::{GraphControl, RenderSource, MAX_CHANNELS},
    routing::target,
    Status,
};
use rodio::{
    cpal::{
        self,
        traits::{DeviceTrait, HostTrait},
    },
    OutputStream, OutputStreamBuilder, Sink, Source,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct Output {
    device: Option<OutputStream>,
    lost: Arc<AtomicBool>,
    sink: Option<Sink>,
    retiring: Option<(Sink, Arc<crate::pause::PauseControl>)>,
    pub clock: Option<Clock>,
    control: Option<GraphControl>,
}
fn device_config() -> Result<(cpal::Device, cpal::SupportedStreamConfig), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("Audio output device is unavailable.")?;
    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let mut best = default.clone();
    if let Ok(configs) = device.supported_output_configs() {
        for config in configs {
            let n = config.channels();
            if usize::from(n) <= MAX_CHANNELS && n > best.channels() {
                let rate = default
                    .sample_rate()
                    .clamp(config.min_sample_rate(), config.max_sample_rate());
                best = config.with_sample_rate(rate);
            }
        }
    }
    Ok((device, best))
}
pub fn device_channels() -> Result<u16, String> {
    device_config().map(|(_, c)| c.channels())
}

impl Output {
    pub fn attach(
        &mut self,
        source: PcmSource,
        clock: Clock,
        volume: f32,
        paused: bool,
        settings: &Processing,
    ) -> Result<(), String> {
        if self.device.is_none() {
            self.lost = Arc::new(AtomicBool::new(false));
            let lost = self.lost.clone();
            let (hardware, config) = device_config()?;
            let builder = OutputStreamBuilder::from_device(hardware)
                .map_err(|e| format!("Audio output device is unavailable: {e}"))?;
            let mut device = builder
                .with_supported_config(&config)
                .with_error_callback(move |_| lost.store(true, Ordering::Release))
                .open_stream_or_fallback()
                .map_err(|e| format!("Audio output failed: {e}"))?;
            device.log_on_drop(false);
            self.device = Some(device);
        }
        let device = self.device.as_ref().unwrap();
        let channels = device.config().channel_count();
        if usize::from(channels) > MAX_CHANNELS || usize::from(source.channels()) > MAX_CHANNELS {
            return Err("Audio output supports up to 32 channels.".into());
        }
        let (source, control) = RenderSource::new(source, channels, settings, paused);
        self.control = Some(control);
        let sink = Sink::connect_new(device.mixer());
        sink.set_volume(volume);
        if paused || self.retiring.is_some() {
            sink.pause();
        }
        sink.append(source);
        self.clock = Some(clock);
        self.sink = Some(sink);
        Ok(())
    }

    pub fn detach(&mut self, duration_ms: u64) {
        if duration_ms == 0 {
            self.retiring = None;
        }
        if let (Some(sink), Some(clock), Some(control)) =
            (self.sink.take(), self.clock.take(), self.control.take())
        {
            if duration_ms > 0 && !sink.is_paused() && !sink.empty() && clock.started() {
                clock
                    .fades
                    .retiring_ms
                    .store(duration_ms, Ordering::Release);
                self.retiring = Some((sink, control.pause));
            }
        }
    }
    pub fn configure(&self, settings: &Processing) {
        if let Some(control) = &self.control {
            control.update(settings);
        }
    }
    pub fn update_fades(
        &mut self,
        settings: crate::FadeSettings,
        track_tail_ms: u64,
        paused: bool,
    ) {
        if let Some(clock) = &self.clock {
            clock
                .fades
                .track_tail_ms
                .store(track_tail_ms, Ordering::Release);
            if !settings.enabled {
                clock.fades.incoming_ms.store(0, Ordering::Relaxed);
            }
        }
        if !settings.enabled {
            self.retiring = None;
        }
        if settings.duration(settings.pause_ms) == 0 {
            if paused {
                self.pause(0);
            } else {
                self.resume(0);
            }
        }
    }
    pub fn capabilities(&self, config: &Processing) -> OutputStatus {
        let max_channels = self.device.as_ref().map(|d| d.config().channel_count());
        if let Some(control) = &self.control {
            let logical = target(config.routing, control.source, control.device);
            OutputStatus {
                max_channels,
                source_channels: Some(control.source),
                output_channels: Some(logical),
                limited: logical != config.routing.layout.channels(control.source),
            }
        } else {
            OutputStatus {
                max_channels,
                ..Default::default()
            }
        }
    }

    pub fn poll(&mut self, paused: bool) -> Result<(), String> {
        if self.lost.swap(false, Ordering::AcqRel) {
            return Err("Audio output device disconnected.".into());
        }
        if self
            .retiring
            .as_ref()
            .is_some_and(|(sink, gate)| sink.empty() || gate.silent.load(Ordering::Acquire))
        {
            self.retiring = None;
            if !paused {
                if let Some(sink) = &self.sink {
                    sink.play();
                }
            }
        }
        if paused
            && self
                .control
                .as_ref()
                .is_some_and(|c| c.pause.silent.load(Ordering::Acquire))
        {
            if let Some(sink) = &self.sink {
                sink.pause();
            }
        }
        Ok(())
    }

    pub fn resume(&self, ms: u64) {
        if let (Some(sink), Some(control)) = (&self.sink, &self.control) {
            control.pause.resume(ms, sink.is_paused());
            if self.retiring.is_none() {
                sink.play();
            }
        }
    }

    pub fn pause(&mut self, ms: u64) {
        if ms == 0 {
            self.retiring = None;
            if let Some(sink) = &self.sink {
                sink.pause();
            }
        }
        if let Some((_, gate)) = &self.retiring {
            gate.pause(ms);
        }
        if let Some(control) = &self.control {
            control.pause.pause(ms);
        }
    }

    pub fn set_volume(&self, volume: f32) {
        for sink in self
            .sink
            .iter()
            .chain(self.retiring.iter().map(|(sink, _)| sink))
        {
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
