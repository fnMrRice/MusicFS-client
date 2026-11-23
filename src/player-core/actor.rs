use crate::decode::{Event, Lifecycle, Worker};
use crate::output::Output;
use crate::{
    FadeSettings, Operation, PlayerCommand, PlayerSnapshot, Presentation, Request, Status, Track,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
#[path = "../../tests/native/fades.rs"]
mod fade_tests;

pub fn run(receiver: Receiver<Request>, decoder: Worker) {
    let mut player = Actor::new(decoder);
    loop {
        player.poll();
        match receiver.recv_timeout(Duration::from_millis(30)) {
            Ok(Request::Full(op, reply)) => {
                let result = player.apply(op).map(|_| player.state.clone());
                let _ = reply.send(result);
            }
            Ok(Request::Presentation(reply)) => {
                let _ = reply.send(player.presentation());
            }
            Err(RecvTimeoutError::Timeout) => (),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    player.stop_output();
}

struct Actor {
    instance: u64,
    state: PlayerSnapshot,
    decoder: Worker,
    generation: u64,
    revision: u64,
    seeks: Option<Sender<(u64, u64)>>,
    lifecycle: Option<Lifecycle>,
    output: Output,
    fades: FadeSettings,
    fade_in: bool,
    paused: bool,
    processing: crate::audio::Processing,
    duration_ms: u64,
}

impl Actor {
    fn new(decoder: Worker) -> Self {
        static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
        Self {
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            state: PlayerSnapshot::default(),
            decoder,
            generation: 0,
            revision: 0,
            seeks: None,
            lifecycle: None,
            output: Output::default(),
            fades: FadeSettings::default(),
            fade_in: false,
            paused: false,
            processing: Default::default(),
            duration_ms: crate::fade::FADE_MS,
        }
    }
    fn presentation(&self) -> Presentation {
        let track = self.state.index.and_then(|i| self.state.queue.get(i));
        Presentation {
            instance: self.instance,
            generation: self.generation,
            title: track.map(|t| t.title.clone()),
            artists: track.map(|t| t.artists.clone()).unwrap_or_default(),
            source_id: self.state.active_source_track_id.clone(),
            status: self.state.status,
            position_ms: self.state.position_ms,
            duration_ms: self.state.duration_ms,
            has_next: self.has_next(),
            error: self.state.error.clone(),
        }
    }
    fn has_next(&self) -> bool {
        self.state
            .index
            .is_some_and(|i| i + 1 < self.state.queue.len())
    }
    fn apply(&mut self, op: Operation) -> Result<(), String> {
        match op {
            Operation::Snapshot => Ok(()),
            Operation::Queue(tracks, start) => {
                if tracks.is_empty() {
                    return self.command(PlayerCommand::Clear);
                }
                if start >= tracks.len() {
                    return Err("Queue index is out of range.".into());
                }
                if tracks.iter().any(|t| t.id.is_empty()) {
                    return Err("Missing track identifier.".into());
                }
                self.state.queue = Arc::new(tracks);
                self.select(start)
            }
            Operation::Command(command) => self.command(command),
        }
    }

    fn command(&mut self, command: PlayerCommand) -> Result<(), String> {
        match command {
            PlayerCommand::Select { index } => self.select(index),
            PlayerCommand::Play => self.play(),
            PlayerCommand::Pause => {
                self.pause();
                Ok(())
            }
            PlayerCommand::Toggle => {
                if matches!(self.state.status, Status::Playing | Status::Loading) && !self.paused {
                    self.pause();
                    Ok(())
                } else {
                    self.play()
                }
            }
            PlayerCommand::Next => self.advance(false),
            PlayerCommand::Previous => {
                let index = self.state.index.ok_or("Queue is empty.")?;
                self.select(index.saturating_sub(1))
            }
            PlayerCommand::Clear | PlayerCommand::Stop => {
                self.stop_output();
                self.state.status = Status::Idle;
                self.state.position_ms = 0;
                self.state.error = None;
                if matches!(command, PlayerCommand::Clear) {
                    self.state.index = None;
                    self.state.duration_ms = None;
                    self.state.queue = Arc::default();
                }
                Ok(())
            }
            PlayerCommand::Volume { volume } => {
                if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
                    return Err("Volume must be between 0 and 1.".into());
                }
                self.state.volume = volume;
                self.output.set_volume(volume);
                Ok(())
            }
            PlayerCommand::SetFades { settings } => {
                self.fades = settings;
                if let Some(clock) = &self.output.clock {
                    clock
                        .fades
                        .track_tail
                        .store(settings.track_change && self.has_next(), Ordering::Release);
                }
                Ok(())
            }
            PlayerCommand::FadeDuration { ms } => {
                self.duration_ms = ms.clamp(100, crate::fade::MAX_FADE_MS);
                if let Some(clock) = &self.output.clock {
                    clock
                        .fades
                        .duration_ms
                        .store(self.duration_ms, Ordering::Relaxed);
                }
                Ok(())
            }
            PlayerCommand::Processing { mut settings } => {
                settings.dsp.normalize();
                if self.processing != settings {
                    self.output.configure(&settings);
                    self.processing = settings;
                }
                self.state.output = self.output.capabilities(&self.processing);
                Ok(())
            }
            PlayerCommand::Seek { position_ms } => self.seek(position_ms),
        }
    }

    fn stop_output(&mut self) {
        self.reset_media(false, false);
    }

    fn reset_media(&mut self, drain_tail: bool, fade: bool) {
        self.output.detach(fade);
        self.state.output = Default::default();
        self.generation = self.decoder.cancel();
        self.seeks = None;
        if !drain_tail && !fade {
            self.output = Output::default();
        }
        self.fade_in = fade;
        self.lifecycle = None;
        self.state.seekable = false;
        self.state.seek_range_ms = None;
        self.state.active_source_track_id = None;
        self.paused = false;
    }

    fn select(&mut self, index: usize) -> Result<(), String> {
        self.start(index, false)
    }

    fn start(&mut self, index: usize, drain_tail: bool) -> Result<(), String> {
        let track: Track = self
            .state
            .queue
            .get(index)
            .ok_or("Queue index is out of range.")?
            .clone();
        let fade = self.fades.track_change && self.seeks.is_some();
        self.reset_media(drain_tail, fade);
        self.revision = 0;
        self.state.index = Some(index);
        self.state.position_ms = 0;
        self.state.duration_ms = track.duration_ms;
        self.state.status = Status::Loading;
        self.state.error = None;
        match self.decoder.load(self.generation, track) {
            Ok(seeks) => {
                self.seeks = Some(seeks);
                Ok(())
            }
            Err(error) => {
                self.fail(error.clone());
                Err(error)
            }
        }
    }

    fn play(&mut self) -> Result<(), String> {
        let index = self.state.index.ok_or("Queue is empty.")?;
        if self.seeks.is_none() {
            return self.select(index);
        }
        self.paused = false;
        self.output.play();
        self.state.status = self.output.status(false);
        Ok(())
    }

    fn pause(&mut self) {
        if self.seeks.is_none() {
            return;
        }
        self.paused = true;
        self.output.pause();
        self.state.status = Status::Paused;
    }

    fn advance(&mut self, drain_tail: bool) -> Result<(), String> {
        let index = self.state.index.ok_or("Queue is empty.")?;
        if index + 1 < self.state.queue.len() {
            return self.start(index + 1, drain_tail);
        }
        // 曲终时保留输出设备，让系统缓冲区内的末尾样本实际播完。
        self.reset_media(drain_tail, false);
        self.state.status = Status::Ended;
        Ok(())
    }

    fn seek(&mut self, position_ms: u64) -> Result<(), String> {
        if !self.state.seekable {
            return Err("This stream does not support seeking.".into());
        }
        let (start, end) = self.state.seek_range_ms.ok_or("No seekable range.")?;
        if !(start..=end).contains(&position_ms) {
            return Err("Position is outside the seekable range.".into());
        }
        let target = position_ms;
        let revision = self.revision + 1;
        self.seeks
            .as_ref()
            .ok_or("No active playback.")?
            .send((revision, target))
            .map_err(|_| "Decoder is unavailable.")?;
        self.revision = revision;
        self.fade_in = self.fades.seek && !self.paused;
        self.output.detach(self.fade_in);
        self.state.position_ms = target;
        self.state.status = if self.paused {
            Status::Paused
        } else {
            Status::Loading
        };
        Ok(())
    }

    fn fail(&mut self, error: String) {
        self.stop_output();
        self.state.status = Status::Error;
        self.state.error = Some(error);
    }

    fn poll(&mut self) {
        while let Ok((generation, event)) = self.decoder.events.try_recv() {
            if generation == self.generation {
                if let Err(error) = self.handle(event) {
                    self.fail(error);
                }
            }
        }
        if let Err(error) = self.output.poll(self.paused) {
            self.fail(error);
            return;
        }
        if let Some(clock) = &self.output.clock {
            self.state.status = self.output.status(self.paused);
            self.state.position_ms = clock.position_ms();
            if clock.started() {
                if let Some(started) = self.lifecycle.as_mut().and_then(|l| l.started.take()) {
                    started();
                }
            }
        }
        if self.output.ended() {
            if let Err(error) = self.advance(true) {
                self.fail(error);
            }
        }
    }

    fn handle(&mut self, event: Event) -> Result<(), String> {
        match event {
            Event::Failed(error) => Err(error),
            Event::Opened(lifecycle) => {
                self.state.active_source_track_id = Some(lifecycle.source_id.clone());
                self.lifecycle = Some(lifecycle);
                Ok(())
            }
            Event::Ready {
                revision,
                mut source,
                clock,
                duration_ms,
                seekable,
            } => {
                if revision != self.revision {
                    return Ok(());
                }
                source.envelope.fade_in = self.fade_in;
                clock
                    .fades
                    .duration_ms
                    .store(self.duration_ms, Ordering::Relaxed);
                clock.fades.track_tail.store(
                    self.fades.track_change && self.has_next(),
                    Ordering::Release,
                );
                self.output.attach(
                    *source,
                    clock,
                    self.state.volume,
                    self.paused,
                    &self.processing,
                )?;
                self.state.output = self.output.capabilities(&self.processing);
                self.state.seek_range_ms = self
                    .lifecycle
                    .as_ref()
                    .and_then(|l| l.seek_range_ms)
                    .filter(|(start, end)| seekable && start < end);
                self.state.seekable = self.state.seek_range_ms.is_some();
                self.state.duration_ms = duration_ms.or(self.state.duration_ms);
                self.state.status = self.output.status(self.paused);
                Ok(())
            }
        }
    }
}
