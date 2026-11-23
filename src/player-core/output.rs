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
    retiring: Option<Sink>,
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
        let (source, control) = RenderSource::new(source, channels, settings);
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

    pub fn detach(&mut self, fade_out: bool) {
        self.control = None;
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
    pub fn configure(&self, settings: &Processing) {
        if let Some(control) = &self.control {
            control.update(settings);
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
