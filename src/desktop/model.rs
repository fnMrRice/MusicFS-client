use crate::{connection::Connection, task::Task};
use directories::ProjectDirs;
use eframe::egui::Context;
use musicfs_player_core::{FadeSettings, PlayerCommand};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::Duration};
use tokio::runtime::Handle;

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub endpoint: String,
    pub theme: String,
    pub fades: FadeSettings,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8080".into(),
            theme: "dark".into(),
            fades: FadeSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_preferences_default_on_and_round_trip_independently() {
        let old: Preferences = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(old.theme, "light");
        assert!(old.fades.seek && old.fades.track_change);
        for seek in [true, false] {
            for track_change in [true, false] {
                let fades = FadeSettings { seek, track_change };
                let prefs = Preferences {
                    fades,
                    ..Default::default()
                };
                let saved = serde_json::to_vec(&prefs).unwrap();
                assert_eq!(
                    serde_json::from_slice::<Preferences>(&saved).unwrap().fades,
                    fades
                );
            }
        }
    }
}

pub struct DesktopState {
    pub runtime: Handle,
    pub connection: Option<Connection>,
    pub busy: bool,
    pub error: Option<String>,
    pub prefs: Preferences,
    settings_file: Option<PathBuf>,
    transition: Option<Task<Option<Connection>>>,
}

impl DesktopState {
    pub fn new(runtime: Handle) -> Self {
        let settings_file = std::env::var_os("MUSICFS_DESKTOP_CONFIG_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                ProjectDirs::from("io.github", "MusicFS", "MusicFS Desktop")
                    .map(|d| d.config_dir().to_owned())
            })
            .map(|dir| dir.join("preferences.json"));
        let prefs = settings_file
            .as_ref()
            .and_then(|file| fs::read(file).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            runtime,
            connection: None,
            busy: false,
            error: None,
            prefs,
            settings_file,
            transition: None,
        }
    }

    pub fn save_preferences(&self) {
        let Some(path) = &self.settings_file else {
            return;
        };
        let Some(dir) = path.parent() else { return };
        let save = || -> std::io::Result<()> {
            fs::create_dir_all(dir)?;
            let mut file = tempfile::NamedTempFile::new_in(dir)?;
            serde_json::to_writer(file.as_file_mut(), &self.prefs)?;
            file.persist(path)?;
            Ok(())
        };
        if save().is_err() {
            eprintln!("Could not save desktop preferences.");
        }
    }

    pub fn connect(&mut self, endpoint: String, token: String, ctx: &Context) {
        if self.busy || self.connection.is_some() {
            return;
        }
        self.busy = true;
        self.error = None;
        self.transition = Some(Task::spawn(&self.runtime, ctx, async move {
            Connection::open(endpoint, token).await.map(Some)
        }));
    }

    pub fn disconnect(&mut self, ctx: &Context) {
        let Some(connection) = self.connection.take() else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.transition = Some(Task::spawn(&self.runtime, ctx, async move {
            connection.close().await.map(|()| None)
        }));
    }

    pub fn update(&mut self) {
        if let Some(result) = self.transition.as_mut().and_then(Task::poll) {
            self.transition = None;
            self.busy = false;
            match result {
                Ok(connection) => {
                    if let Some(active) = &connection {
                        if let Err(error) = active.player.command(PlayerCommand::SetFades {
                            settings: self.prefs.fades,
                        }) {
                            self.error = Some(error);
                        }
                        self.prefs.endpoint.clone_from(&active.server.info.endpoint);
                        self.save_preferences();
                    }
                    self.connection = connection;
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    pub fn shutdown(&mut self) {
        let pending = self.transition.take();
        let connection = self.connection.take();
        self.runtime.block_on(async {
            let _ = tokio::time::timeout(Duration::from_secs(4), async {
                let connection = match pending {
                    Some(task) => task.finish().await.flatten(),
                    None => connection,
                };
                if let Some(connection) = connection {
                    let _ = connection.close().await;
                }
            })
            .await;
        });
    }
}
