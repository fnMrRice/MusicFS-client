use crate::audio::{Processing, DSP_BUFFER_FRAMES};
use crate::filters::Unit;
use crate::level::Level;
use crate::pause::{PauseControl, PauseEnvelope};
use crate::pcm::PcmSource;
use crate::routing::{target, Matrix};
use rodio::Source;
use std::sync::mpsc::TryRecvError;
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
struct Pending {
    settings: Processing,
    graph: Option<Graph>,
}

#[derive(Clone)]
pub struct GraphControl {
    pending: Arc<Mutex<Pending>>,
    pub source: u16,
    pub device: u16,
    pub pause: Arc<PauseControl>,
    rate: u32,
}
impl GraphControl {
    fn new(
        source: u16,
        device: u16,
        rate: u32,
        settings: &Processing,
        pause: Arc<PauseControl>,
    ) -> Self {
        Self {
            pending: Arc::new(Mutex::new(Pending {
                settings: settings.clone(),
                graph: None,
            })),
            source,
            device,
            pause,
            rate,
        }
    }
    pub fn update(&self, config: &Processing) {
        if let Ok(mut pending) = self.pending.lock() {
            // 只改帧数时保留滤波器和混响的历史状态。
            pending.settings.dsp.buffer_frames = config.dsp.buffer_frames;
            if pending.settings != *config {
                pending.graph = Some(Graph::new(config, self.source, self.device, self.rate));
                pending.settings = config.clone();
            }
        }
    }
}

struct GraphTransition {
    graph: Graph,
    previous: Option<Graph>,
    fade: u32,
    remaining: u32,
}

impl GraphTransition {
    fn new(config: &GraphControl, settings: &Processing) -> Self {
        Self {
            graph: Graph::new(settings, config.source, config.device, config.rate),
            previous: None,
            fade: (config.rate / 100).max(1),
            remaining: 0,
        }
    }

    fn replace(&mut self, graph: Graph) {
        self.previous = Some(std::mem::replace(&mut self.graph, graph));
        self.remaining = self.fade;
    }

    fn apply(&mut self, input: &[f32], out: &mut [f32]) {
        let mut source = [0.; MAX_CHANNELS];
        let source = &mut source[..input.len()];
        source.copy_from_slice(input);
        self.graph.apply(source, out);
        if let Some(previous) = &mut self.previous {
            source.copy_from_slice(input);
            let mut old = [0.; MAX_CHANNELS];
            previous.apply(source, &mut old[..out.len()]);
            let wet = 1. - self.remaining as f32 / self.fade as f32;
            for (v, dry) in out.iter_mut().zip(old) {
                *v = dry + (*v - dry) * wet;
            }
            self.remaining -= 1;
            if self.remaining == 0 {
                self.previous = None;
            }
        }
    }
}

// 按设置分块处理，硬件逐帧取出。保留本块输入，使切换输出声道时可重做剩余帧的路由。
struct DspBlock {
    raw: Vec<f32>,
    output: Vec<f32>,
    size: usize,
    len: usize,
    cursor: usize,
    has_media: bool,
}

impl DspBlock {
    fn storage(channels: u16) -> Vec<f32> {
        vec![0.; usize::from(channels) * DSP_BUFFER_FRAMES[DSP_BUFFER_FRAMES.len() - 1]]
    }

    fn new(config: &GraphControl, size: usize) -> Self {
        Self {
            raw: Self::storage(config.source),
            output: Self::storage(config.device),
            size,
            len: 0,
            cursor: 0,
            has_media: false,
        }
    }

    fn process(&mut self, config: &GraphControl, graph: &mut GraphTransition) {
        let source = usize::from(config.source);
        let device = usize::from(config.device);
        let raw = &self.raw[self.cursor * source..self.len * source];
        let output = &mut self.output[self.cursor * device..self.len * device];
        for (src, out) in raw
            .chunks_exact(source)
            .zip(output.chunks_exact_mut(device))
        {
            graph.apply(src, out);
        }
    }
}

pub struct RenderSource {
    input: PcmSource,
    pause: PauseEnvelope,
    config: GraphControl,
    graph: GraphTransition,
    block: DspBlock,
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
        let config = GraphControl::new(source, device, rate, settings, pause.control.clone());
        let graph = GraphTransition::new(&config, settings);
        let block = DspBlock::new(&config, settings.dsp.block_frames());
        (
            Self {
                input,
                pause,
                config: config.clone(),
                graph,
                block,
                frame: [0.; MAX_CHANNELS],
                pos: device as usize,
            },
            config,
        )
    }

    pub fn change_device(
        &mut self,
        channels: u16,
        settings: &Processing,
        paused: bool,
    ) -> GraphControl {
        self.pause = PauseEnvelope::new(self.config.rate, paused);
        if channels != self.config.device {
            self.config = GraphControl::new(
                self.config.source,
                channels,
                self.config.rate,
                settings,
                self.pause.control.clone(),
            );
            self.graph = GraphTransition::new(&self.config, settings);
            self.block.output = DspBlock::storage(channels);
            self.block.size = settings.dsp.block_frames();
            self.block.process(&self.config, &mut self.graph);
        } else {
            self.config.pause = self.pause.control.clone();
            self.config.update(settings);
        }
        self.pos = usize::from(channels);
        self.config.clone()
    }

    fn refill(&mut self) -> Option<()> {
        if let Ok(mut pending) = self.config.pending.try_lock() {
            self.block.size = pending.settings.dsp.block_frames();
            if let Some(graph) = pending.graph.take() {
                self.graph.replace(graph);
            }
        }
        let channels = usize::from(self.config.source);
        let raw = &mut self.block.raw[..self.block.size * channels];
        self.block.has_media = true;
        self.block.len = match self.input.read_frames(raw) {
            Ok(frames) => frames,
            Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => {
                // 欠载只渲染一帧静音，下一帧立即尝试恢复。
                raw[..channels].fill(0.);
                self.block.has_media = false;
                1
            }
        };
        self.block.cursor = 0;
        self.block.process(&self.config, &mut self.graph);
        Some(())
    }

    fn render_frame(&mut self) -> Option<()> {
        if self.pause.holding() {
            self.frame.fill(0.);
            self.pos = 0;
            return Some(());
        }
        if self.block.cursor == self.block.len {
            self.refill()?;
        }
        let gain = self.input.emit_frame(self.block.has_media)?
            * self.pause.next_gain(self.block.has_media);
        let channels = usize::from(self.config.device);
        let start = self.block.cursor * channels;
        for (v, sample) in self.frame[..channels]
            .iter_mut()
            .zip(&self.block.output[start..start + channels])
        {
            *v = (*sample * gain).clamp(-1., 1.);
        }
        self.block.cursor += 1;
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
