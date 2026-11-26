mod commands;

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

#[cfg(test)]
#[path = "../../tests/native/active_format.rs"]
mod active_format_tests;

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
    output_device: Option<String>,
    fades: FadeSettings,
    fade_in_ms: u64,
    paused: bool,
    processing: crate::audio::Processing,
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
            output_device: None,
            fades: FadeSettings::default(),
            fade_in_ms: 0,
            paused: false,
            processing: Default::default(),
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
    fn track_tail_ms(&self) -> u64 {
        if self.has_next() {
            self.fades.duration(self.fades.track_ms)
        } else {
            0
        }
    }
    fn apply(&mut self, op: Operation) -> Result<(), String> {
        match op {
            Operation::Snapshot => Ok(()),
            Operation::Enqueue { tracks, next } => {
                if tracks.iter().any(|track| track.id.is_empty()) {
                    return Err("Missing track identifier.".into());
                }
                let Some(first) = tracks.first() else {
                    return Ok(());
                };
                let queue = Arc::make_mut(&mut self.state.queue);
                if let Some(index) = self.state.index {
                    let at = if next { index + 1 } else { queue.len() };
                    queue.splice(at..at, tracks);
                } else {
                    self.state.index = Some(0);
                    self.state.duration_ms = first.duration_ms;
                    queue.extend(tracks);
                }
                if let Some(clock) = &self.output.clock {
                    clock
                        .fades
                        .track_tail_ms
                        .store(self.track_tail_ms(), Ordering::Release);
                }
                Ok(())
            }
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

    fn stop_output(&mut self) {
        self.reset_media(false, 0);
    }

    fn reset_media(&mut self, drain_tail: bool, fade_ms: u64) {
        self.output.detach(fade_ms);
        self.state.output = Default::default();
        self.generation = self.decoder.cancel();
        self.seeks = None;
        if !drain_tail && fade_ms == 0 {
            self.output = Output::default();
        }
        self.fade_in_ms = fade_ms;
        self.lifecycle = None;
        self.state.seekable = false;
        self.state.seek_range_ms = None;
        self.state.active_source_track_id = None;
        self.state.active_format = None;
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
        let fade = if self.seeks.is_some() {
            self.fades.duration(self.fades.track_ms)
        } else {
            0
        };
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
        if self.paused {
            let ms = self.fades.duration(self.fades.pause_ms);
            if self.output.clock.is_none() {
                self.fade_in_ms = ms;
            }
            self.output.resume(ms);
        }
        self.paused = false;
        self.state.status = self.output.status(false);
        Ok(())
    }

    fn pause(&mut self) {
        if self.seeks.is_none() || self.paused {
            return;
        }
        self.paused = true;
        self.output.pause(self.fades.duration(self.fades.pause_ms));
        self.state.status = Status::Paused;
    }

    fn advance(&mut self, drain_tail: bool) -> Result<(), String> {
        let index = self.state.index.ok_or("Queue is empty.")?;
        if index + 1 < self.state.queue.len() {
            return self.start(index + 1, drain_tail);
        }
        // 曲终时保留输出设备，让系统缓冲区内的末尾样本实际播完。
        self.reset_media(drain_tail, 0);
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
        self.fade_in_ms = if self.paused {
            0
        } else {
            self.fades.duration(self.fades.seek_ms)
        };
        self.output.detach(self.fade_in_ms);
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
        if !self.paused && self.output.ended() {
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
                self.state.active_format = Some(lifecycle.format.clone());
                self.lifecycle = Some(lifecycle);
                Ok(())
            }
            Event::Ready {
                revision,
                source,
                clock,
                duration_ms,
                seekable,
            } => {
                if revision != self.revision {
                    return Ok(());
                }
                clock
                    .fades
                    .incoming_ms
                    .store(self.fade_in_ms, Ordering::Relaxed);
                clock
                    .fades
                    .track_tail_ms
                    .store(self.track_tail_ms(), Ordering::Release);
                self.output.attach(
                    *source,
                    clock,
                    self.state.volume,
                    self.paused,
                    &self.processing,
                    self.output_device.as_deref(),
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
