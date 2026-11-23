use crate::connection::Connection;
use musicfs_player_core::{
    audio::{LocalDsp, Processing, Routing},
    FadeSettings, PlayerCommand,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AudioPreferences {
    pub volume: f32,
    pub last_volume: f32,
    pub force_processing: bool,
    pub fade_ms: u64,
    pub processing: Processing,
}
impl Default for AudioPreferences {
    fn default() -> Self {
        Self {
            volume: 0.8,
            last_volume: 0.8,
            force_processing: false,
            fade_ms: 300,
            processing: Default::default(),
        }
    }
}
impl AudioPreferences {
    pub fn normalize(&mut self) {
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {
            self.volume = 0.8;
        }
        if self.volume > 0. {
            self.last_volume = self.volume;
        } else if !self.last_volume.is_finite() || self.last_volume <= 0. || self.last_volume > 1. {
            self.last_volume = 0.8;
        }
        self.fade_ms = self.fade_ms.clamp(100, 2000);
        self.processing.dsp.normalize();
    }
    pub fn initialize(&self, c: &Connection, fades: FadeSettings) -> Result<(), String> {
        c.force_processing
            .store(self.force_processing, Ordering::Release);
        for command in [
            PlayerCommand::Volume {
                volume: self.volume,
            },
            PlayerCommand::SetFades { settings: fades },
            PlayerCommand::FadeDuration { ms: self.fade_ms },
            PlayerCommand::Processing {
                settings: self.processing.clone(),
            },
        ] {
            c.player.command(command)?;
        }
        Ok(())
    }
    pub fn apply(&mut self, c: &Connection, action: AudioAction) -> Result<(), String> {
        match action {
            AudioAction::Volume(volume) => {
                c.player.command(PlayerCommand::Volume { volume })?;
                self.volume = volume;
                if volume > 0. {
                    self.last_volume = volume;
                }
            }
            AudioAction::Force(value) => {
                c.force_processing.store(value, Ordering::Release);
                self.force_processing = value;
            }
            AudioAction::Fade(ms) => {
                let ms = ms.clamp(100, 2000);
                c.player.command(PlayerCommand::FadeDuration { ms })?;
                self.fade_ms = ms;
            }
            AudioAction::Level(value) => {
                c.player
                    .command(PlayerCommand::AutoVolume { enabled: value })?;
                self.processing.auto_volume = value;
            }
            AudioAction::SaveDsp(value) => self.processing.dsp = value,
            AudioAction::SaveRouting(value) => self.processing.routing = value,
        }
        Ok(())
    }
}
pub enum AudioAction {
    Volume(f32),
    Force(bool),
    Fade(u64),
    Level(bool),
    SaveDsp(LocalDsp),
    SaveRouting(Routing),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_muted_volume_and_local_effects_survive_a_restart() {
        let mut prefs = AudioPreferences {
            volume: 0.,
            last_volume: 0.37,
            force_processing: true,
            fade_ms: 1200,
            ..Default::default()
        };
        prefs
            .processing
            .dsp
            .chain
            .push(musicfs_player_core::audio::Effect::new(
                musicfs_player_core::audio::EffectKind::Compressor,
            ));
        prefs.processing.routing.layout = musicfs_player_core::audio::Layout::Surround51;
        prefs.processing.auto_volume = true;
        let mut restored: AudioPreferences =
            serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();
        restored.normalize();
        assert_eq!(restored.last_volume, 0.37);
        assert_eq!(restored.volume, 0.);
        assert_eq!(restored.processing, prefs.processing);
        assert!(restored.force_processing);
        assert_eq!(restored.fade_ms, 1200);
    }
}
