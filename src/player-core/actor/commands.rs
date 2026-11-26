use super::Actor;
use crate::{PlayerCommand, Status};
use std::sync::Arc;

impl Actor {
    pub(super) fn command(&mut self, command: PlayerCommand) -> Result<(), String> {
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
            PlayerCommand::OutputDevice { device } => {
                if self.output_device != device {
                    let had_source = self.output.clock.is_some();
                    if let Err(error) = self.output.switch_device(
                        device.as_deref(),
                        self.state.volume,
                        self.paused,
                        &self.processing,
                    ) {
                        if had_source && self.output.clock.is_none() {
                            self.fail(error.clone());
                        }
                        return Err(error);
                    }
                    self.output_device = device;
                    self.state.output = self.output.capabilities(&self.processing);
                }
                Ok(())
            }
            PlayerCommand::SetFades { mut settings } => {
                settings.normalize();
                self.fades = settings;
                self.output
                    .update_fades(settings, self.track_tail_ms(), self.paused);
                if !settings.enabled {
                    self.fade_in_ms = 0;
                }
                Ok(())
            }
            PlayerCommand::AutoVolume { enabled } => {
                let mut settings = self.processing.clone();
                settings.auto_volume = enabled;
                self.command(PlayerCommand::Processing { settings })
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
}
