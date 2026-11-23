use crate::audio::Processing;
use crate::filters::Unit;
use crate::level::Level;
use crate::pause::{PauseControl, PauseEnvelope};
use crate::pcm::PcmSource;
use crate::routing::{target, Matrix};
use rodio::Source;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(test)]
#[path = "../../tests/native/local_audio.rs"]
mod tests;

pub const MAX_CHANNELS: usize = 32;
pub type Frame = [f32; MAX_CHANNELS];

pub struct Graph {
    units: Vec<Unit>,
    route: Matrix,
    speakers: Matrix,
    level: Option<Level>,
    logical: usize,
}
impl Graph {
    pub fn new(settings: &Processing, source: u16, device: u16, rate: u32) -> Self {
        let logical = target(settings.routing, source, device) as usize;
        let units = settings
            .dsp
            .chain
            .iter()
            .filter(|e| settings.dsp.enabled && e.enabled)
            .map(|e| Unit::new(e, rate, source as usize))
            .collect();
        Self {
            units,
            route: Matrix::new(
                source as usize,
                logical,
                settings.routing.expand_stereo,
                rate,
            ),
            speakers: Matrix::new(logical, device as usize, false, rate),
            logical,
            level: settings.auto_volume.then(|| Level::new(logical, rate)),
        }
    }
    pub fn apply(&mut self, src: &mut [f32], out: &mut [f32]) {
        for unit in &mut self.units {
            unit.apply(src);
        }
        let mut bus = [0.; MAX_CHANNELS];
        let bus = &mut bus[..self.logical];
        self.route.apply(src, bus);
        if let Some(level) = &mut self.level {
            level.apply(bus);
        }
        self.speakers.apply(bus, out);
    }
}
#[derive(Clone)]
pub struct GraphControl {
    pending: Arc<Mutex<Option<Graph>>>,
    pub source: u16,
    pub device: u16,
    pub pause: Arc<PauseControl>,
    rate: u32,
}
impl GraphControl {
    pub fn update(&self, config: &Processing) {
        let graph = Graph::new(config, self.source, self.device, self.rate);
        if let Ok(mut pending) = self.pending.lock() {
            *pending = Some(graph);
        }
    }
}
pub struct RenderSource {
    input: PcmSource,
    pause: PauseEnvelope,
    config: GraphControl,
    graph: Graph,
    previous: Option<Graph>,
    fade: u32,
    remaining: u32,
    countdown: u8,
    frame: Frame,
    pos: usize,
}
impl RenderSource {
    pub fn new(
        input: PcmSource,
        device: u16,
        settings: &Processing,
        paused: bool,
    ) -> (Self, GraphControl) {
        let rate = input.sample_rate();
        let source = input.channels();
        let pause = PauseEnvelope::new(rate, paused);
        let config = GraphControl {
            pending: Arc::default(),
            source,
            device,
            pause: pause.control.clone(),
            rate,
        };
        let graph = Graph::new(settings, source, device, rate);
        (
            Self {
                input,
                pause,
                config: config.clone(),
                graph,
                previous: None,
                fade: (rate / 100).max(1),
                remaining: 0,
                countdown: 0,
                frame: [0.; MAX_CHANNELS],
                pos: device as usize,
            },
            config,
        )
    }
    fn render_frame(&mut self) -> Option<()> {
        if self.pause.holding() {
            self.frame.fill(0.);
            self.pos = 0;
            return Some(());
        }
        let played = self.input.played_samples();
        let mut input = [0.; MAX_CHANNELS];
        let channels = self.config.source as usize;
        for v in &mut input[..channels] {
            *v = self.input.next_raw()?;
        }
        let pause_gain = self.pause.next_gain(self.input.played_samples() != played);
        if self.countdown == 0 {
            if let Ok(mut pending) = self.config.pending.try_lock() {
                if let Some(graph) = pending.take() {
                    self.previous = Some(std::mem::replace(&mut self.graph, graph));
                    self.remaining = self.fade;
                }
            }
            self.countdown = 64;
        }
        self.countdown -= 1;
        let mut old_input = input;
        let out = &mut self.frame[..self.config.device as usize];
        self.graph.apply(&mut input[..channels], out);
        if let Some(previous) = &mut self.previous {
            let mut old = [0.; MAX_CHANNELS];
            previous.apply(&mut old_input[..channels], &mut old[..out.len()]);
            let wet = 1. - self.remaining as f32 / self.fade as f32;
            for (v, dry) in out.iter_mut().zip(old) {
                *v = dry + (*v - dry) * wet;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.previous = None;
            }
        }
        for v in out {
            *v = (*v * self.input.gain() * pause_gain).clamp(-1., 1.);
        }
        self.pos = 0;
        Some(())
    }
}
impl Iterator for RenderSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos == self.config.device as usize {
            self.render_frame()?;
        }
        let v = self.frame[self.pos];
        self.pos += 1;
        Some(v)
    }
}
impl Source for RenderSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.config.device
    }
    fn sample_rate(&self) -> u32 {
        self.config.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
