use crate::pcm::{Clock, PcmSource};
use crate::{
    audio::{OutputStatus, Processing},
    devices::{resolve, stream_config},
    processing::{GraphControl, RenderSource, MAX_CHANNELS},
    routing::target,
    Status,
};
use rodio::{cpal::Device, OutputStream, OutputStreamBuilder, Sink, Source};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
#[path = "../../tests/native/output_devices.rs"]
mod tests;

// 流释放时归还解码队列和 DSP 缓冲，实时采样路径无需加锁，也无需重新定位媒体。
struct StreamSource {
    source: Option<Box<RenderSource>>,
    release: Sender<Box<RenderSource>>,
}

impl StreamSource {
    fn append(sink: &Sink, source: Box<RenderSource>) -> Receiver<Box<RenderSource>> {
        let (release, receiver) = channel();
        sink.append(Self {
            source: Some(source),
            release,
        });
        receiver
    }
}

impl Iterator for StreamSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        self.source.as_mut()?.next()
    }
}

impl Source for StreamSource {
    fn channels(&self) -> u16 {
        self.source.as_ref().unwrap().channels()
    }
    fn sample_rate(&self) -> u32 {
        self.source.as_ref().unwrap().sample_rate()
    }
    fn current_span_len(&self) -> Option<usize> {
        self.source.as_ref().unwrap().current_span_len()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.as_ref().unwrap().total_duration()
    }
}

impl Drop for StreamSource {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            let _ = self.release.send(source);
        }
    }
}

#[derive(Default)]
pub struct Output {
    device: Option<DeviceStream>,
    sink: Option<Sink>,
    source_return: Option<Receiver<Box<RenderSource>>>,
    retiring: Option<(Sink, Arc<crate::pause::PauseControl>)>,
    pub clock: Option<Clock>,
    control: Option<GraphControl>,
}
struct DeviceStream {
    stream: OutputStream,
    lost: Arc<AtomicBool>,
    #[cfg(any(windows, target_os = "macos"))]
    hardware: Device,
    #[cfg(not(any(windows, target_os = "macos")))]
    selection: Option<String>,
}
impl DeviceStream {
    fn open(hardware: Device, _device_id: Option<&str>) -> Result<Self, String> {
        let config = stream_config(&hardware)?;
        let lost = Arc::new(AtomicBool::new(false));
        let flag = lost.clone();
        let mut stream = OutputStreamBuilder::from_device(hardware.clone())
            .map_err(|e| format!("Audio output device is unavailable: {e}"))?
            .with_supported_config(&config)
            .with_error_callback(move |_| flag.store(true, Ordering::Release))
            .open_stream_or_fallback()
            .map_err(|e| format!("Audio output failed: {e}"))?;
        stream.log_on_drop(false);
        if usize::from(stream.config().channel_count()) > MAX_CHANNELS {
            return Err("Audio output supports up to 32 channels.".into());
        }
        Ok(Self {
            stream,
            lost,
            #[cfg(any(windows, target_os = "macos"))]
            hardware,
            #[cfg(not(any(windows, target_os = "macos")))]
            selection: _device_id.map(str::to_owned),
        })
    }
    fn matches(&self, _hardware: &Device, _device_id: Option<&str>) -> bool {
        #[cfg(any(windows, target_os = "macos"))]
        return crate::devices::same_stream_device(&self.hardware, _hardware);
        #[cfg(not(any(windows, target_os = "macos")))]
        return self.selection.as_deref() == _device_id;
    }
}

impl Output {
    pub fn switch_device(
        &mut self,
        device_id: Option<&str>,
        volume: f32,
        paused: bool,
        settings: &Processing,
    ) -> Result<(), String> {
        let Some(current) = &self.device else {
            return Ok(());
        };
        let hardware = resolve(device_id)?;
        if current.matches(&hardware, device_id) {
            return Ok(());
        }
        // 先打开目标设备。打开失败时，旧设备上的音源继续播放。
        let replacement = DeviceStream::open(hardware, device_id)?;
        let channels = replacement.stream.config().channel_count();
        let receiver = self.source_return.take();
        let clock = self.clock.take();
        *self = Self {
            device: Some(replacement),
            ..Self::default()
        };
        if let (Some(receiver), Some(clock)) = (receiver, clock) {
            let mut source = receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| "Audio source was not released.".to_owned())?;
            let control = source.change_device(channels, settings, paused);
            self.connect(source, control, clock, volume, paused);
        }
        Ok(())
    }

    pub fn attach(
        &mut self,
        source: PcmSource,
        clock: Clock,
        volume: f32,
        paused: bool,
        settings: &Processing,
        device_id: Option<&str>,
    ) -> Result<(), String> {
        if self.device.is_none() || device_id.is_none() {
            let hardware = resolve(device_id)?;
            if self
                .device
                .as_ref()
                .is_none_or(|d| !d.matches(&hardware, device_id))
            {
                let device = DeviceStream::open(hardware, device_id)?;
                *self = Self {
                    device: Some(device),
                    ..Self::default()
                };
            }
        }
        let device = self.device.as_ref().unwrap();
        let channels = device.stream.config().channel_count();
        if usize::from(source.channels()) > MAX_CHANNELS {
            return Err("Audio output supports up to 32 channels.".into());
        }
        let (source, control) = RenderSource::new(source, channels, settings, paused);
        self.connect(Box::new(source), control, clock, volume, paused);
        Ok(())
    }

    fn connect(
        &mut self,
        source: Box<RenderSource>,
        control: GraphControl,
        clock: Clock,
        volume: f32,
        paused: bool,
    ) {
        self.control = Some(control);
        let sink = Sink::connect_new(self.device.as_ref().unwrap().stream.mixer());
        sink.set_volume(volume);
        if paused || self.retiring.is_some() {
            sink.pause();
        }
        self.source_return = Some(StreamSource::append(&sink, source));
        self.clock = Some(clock);
        self.sink = Some(sink);
    }

    pub fn detach(&mut self, duration_ms: u64) {
        self.source_return = None;
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
        let max_channels = self
            .device
            .as_ref()
            .map(|d| d.stream.config().channel_count());
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
        if self
            .device
            .as_ref()
            .is_some_and(|d| d.lost.swap(false, Ordering::AcqRel))
        {
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
