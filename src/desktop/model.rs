use crate::{connection::Connection, task::Task};
use directories::ProjectDirs;
use eframe::egui::Context;
use musicfs_player_core::FadeSettings;
#[path = "model/audio.rs"]
pub mod audio;
#[path = "model/credentials.rs"]
pub(crate) mod credentials;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::runtime::Handle;

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub endpoint: String,
    pub theme: String,
    pub accent: String,
    pub language: String,
    pub fades: FadeSettings,
    pub audio: audio::AudioPreferences,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_login: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    forgotten_logins: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8080".into(),
            theme: "dark".into(),
            accent: "default".into(),
            language: "system".into(),
            fades: FadeSettings::default(),
            audio: Default::default(),
            saved_login: None,
            forgotten_logins: vec![],
        }
    }
}

impl Preferences {
    fn decode(bytes: &[u8]) -> serde_json::Result<Self> {
        let saved: serde_json::Value = serde_json::from_slice(bytes)?;
        // 旧配置将两个开关和公共时长分别保存在 fades 与 audio 中。
        let old_flags = &saved["fades"];
        let old_ms = saved["audio"]["fade_ms"]
            .as_u64()
            .unwrap_or(300)
            .clamp(100, 2000);
        let legacy = old_flags.get("enabled").is_none()
            && (old_flags.get("seek").is_some()
                || old_flags.get("track_change").is_some()
                || saved["audio"].get("fade_ms").is_some());
        let track = old_flags["track_change"].as_bool().unwrap_or(true);
        let seek = old_flags["seek"].as_bool().unwrap_or(true);
        let mut prefs: Self = serde_json::from_value(saved)?;
        if legacy {
            let enabled = track || seek;
            prefs.fades = FadeSettings {
                enabled,
                track_ms: if track || !enabled { old_ms } else { 0 },
                seek_ms: if seek || !enabled { old_ms } else { 0 },
                ..Default::default()
            };
        }
        prefs.fades.normalize();
        Ok(prefs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_preferences_keep_old_choices_and_save_each_duration() {
        let old = Preferences::decode(br#"{"theme":"light"}"#).unwrap();
        assert_eq!(old.theme, "light");
        assert_eq!(old.fades, FadeSettings::default());
        for seek in [true, false] {
            for track_change in [true, false] {
                let legacy = serde_json::json!({"audio":{"fade_ms":650}, "fades":{ "seek":seek, "track_change":track_change }});
                let mut prefs = Preferences::decode(&serde_json::to_vec(&legacy).unwrap()).unwrap();
                assert_eq!(prefs.fades.enabled, seek || track_change);
                assert_eq!(
                    prefs.fades.seek_ms,
                    if seek || !track_change { 650 } else { 0 }
                );
                assert_eq!(
                    prefs.fades.track_ms,
                    if track_change || !seek { 650 } else { 0 }
                );
                assert_eq!(prefs.fades.pause_ms, 300);
                prefs.fades.pause_ms = 120;
                let saved = serde_json::to_vec(&prefs).unwrap();
                assert_eq!(Preferences::decode(&saved).unwrap().fades, prefs.fades);
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
    cleanup: Option<Task<Vec<String>>>,
    initial_restore: bool,
    cleanup_due: bool,
    expiry_check: Instant,
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
        Self::at_settings(runtime, settings_file)
    }
    fn at_settings(runtime: Handle, settings_file: Option<PathBuf>) -> Self {
        let mut prefs: Preferences = settings_file
            .as_ref()
            .and_then(|file| fs::read(file).ok())
            .and_then(|bytes| Preferences::decode(&bytes).ok())
            .unwrap_or_default();
        prefs.audio.normalize();
        Self {
            runtime,
            connection: None,
            busy: false,
            error: None,
            prefs,
            settings_file,
            transition: None,
            cleanup: None,
            initial_restore: true,
            cleanup_due: true,
            expiry_check: Instant::now(),
        }
    }

    pub fn save_preferences(&mut self) {
        if let Err(error) = self.persist_preferences() {
            self.error = Some(error);
        }
    }
    fn persist_preferences(&self) -> Result<(), String> {
        let path = self
            .settings_file
            .as_ref()
            .ok_or("Desktop preferences storage is unavailable.")?;
        let dir = path
            .parent()
            .ok_or("Desktop preferences storage is unavailable.")?;
        let save = || -> std::io::Result<()> {
            fs::create_dir_all(dir)?;
            let mut file = tempfile::NamedTempFile::new_in(dir)?;
            serde_json::to_writer(file.as_file_mut(), &self.prefs)?;
            file.persist(path)?;
            Ok(())
        };
        save().map_err(|error| format!("Could not save desktop preferences: {error}"))
    }

    pub fn connect(&mut self, endpoint: String, token: String, remember: bool, ctx: &Context) {
        if self.busy || self.connection.is_some() {
            return;
        }
        self.busy = true;
        self.error = None;
        self.transition = Some(Task::spawn(&self.runtime, ctx, async move {
            credentials::manual(endpoint, zeroize::Zeroizing::new(token), remember)
                .await
                .map(Some)
        }));
    }

    pub fn restore(&mut self, ctx: &Context) {
        if self.busy || self.connection.is_some() {
            return;
        }
        let Some(slot) = self.prefs.saved_login.clone() else {
            return;
        };
        let endpoint = self.prefs.endpoint.clone();
        self.busy = true;
        self.error = None;
        self.transition = Some(Task::spawn(&self.runtime, ctx, async move {
            credentials::restore(slot, endpoint).await.map(Some)
        }));
    }
    pub fn forget(&mut self) {
        if let Some(slot) = self.prefs.saved_login.take() {
            self.prefs.forgotten_logins.push(slot);
            self.cleanup_due = true;
            self.save_preferences();
        }
    }
    fn clean_forgotten(&mut self, ctx: &Context) {
        if let Some(result) = self.cleanup.as_mut().and_then(Task::poll) {
            self.cleanup = None;
            if let Ok(deleted) = result {
                self.prefs
                    .forgotten_logins
                    .retain(|id| !deleted.contains(id));
                if !self.prefs.forgotten_logins.is_empty() {
                    self.error=Some("Could not remove a saved login from the system credential store. Cleanup will retry on startup.".into());
                }
                self.save_preferences();
            }
        }
        if self.cleanup_due && self.cleanup.is_none() {
            self.cleanup_due = false;
            let mut pending = self.prefs.forgotten_logins.clone();
            pending.retain(|id| self.prefs.saved_login.as_ref() != Some(id));
            if !pending.is_empty() {
                self.cleanup = Some(Task::spawn(&self.runtime, ctx, async move {
                    let mut deleted = vec![];
                    for id in pending {
                        if credentials::forget(id.clone()).await.is_ok() {
                            deleted.push(id);
                        }
                    }
                    Ok(deleted)
                }));
            }
        }
    }

    pub fn disconnect(&mut self, ctx: &Context) {
        let Some(connection) = self.connection.take() else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.forget();
        self.clean_forgotten(ctx);
        self.transition = Some(Task::spawn(&self.runtime, ctx, async move {
            connection.close().await.map(|()| None)
        }));
    }
    pub fn reauthenticate(&mut self, ctx: &Context) {
        if self.busy {
            return;
        }
        if let Some(connection) = self.connection.take() {
            self.runtime.spawn(async move {
                let _ = connection.close().await;
            });
        }
        self.error = Some("Your MusicFS session expired. Sign in again.".into());
        self.restore(ctx);
    }

    pub fn update(&mut self, ctx: &Context) {
        self.clean_forgotten(ctx);
        if self.initial_restore {
            self.initial_restore = false;
            self.restore(ctx);
        }
        if !self.busy && self.expiry_check.elapsed() > Duration::from_secs(1) {
            self.expiry_check = Instant::now();
            if self.connection.as_ref().is_some_and(|c| c.server.expired()) {
                self.reauthenticate(ctx);
            }
        }
        if let Some(result) = self.transition.as_mut().and_then(Task::poll) {
            self.transition = None;
            self.busy = false;
            match result {
                Ok(connection) => {
                    if let Some(active) = connection {
                        self.accept(active);
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
    fn accept(&mut self, mut active: Connection) {
        self.error = active.login_notice.take();
        if let Err(error) = self.prefs.audio.initialize(&active, self.prefs.fades) {
            self.error = Some(error);
        }
        if self.prefs.saved_login != active.saved_login {
            if let Some(old) = self.prefs.saved_login.take() {
                self.prefs.forgotten_logins.push(old);
                self.cleanup_due = true;
            }
            self.prefs.saved_login = active.saved_login.clone();
        }
        self.prefs.endpoint.clone_from(&active.server.info.endpoint);
        match self.persist_preferences() {
            Ok(()) => {
                if let Some(guard) = active.new_login.take() {
                    guard.commit();
                }
            }
            Err(error) => {
                self.error = Some(error);
                if active.new_login.take().is_some() {
                    self.prefs.saved_login = None;
                    active.saved_login = None;
                }
            }
        }
        self.connection = Some(active);
    }

    pub fn shutdown(&mut self) {
        if let Some(task) = self.transition.take() {
            if let Ok(Some(Some(connection))) = self.runtime.block_on(async {
                tokio::time::timeout(Duration::from_secs(4), task.finish()).await
            }) {
                self.accept(connection);
            }
        }
        if let Some(cleanup) = self.cleanup.take() {
            if let Ok(Some(deleted)) = self.runtime.block_on(async {
                tokio::time::timeout(Duration::from_secs(2), cleanup.finish()).await
            }) {
                self.prefs
                    .forgotten_logins
                    .retain(|id| !deleted.contains(id));
                self.save_preferences();
            }
        }
        if let Some(connection) = self.connection.take() {
            self.runtime.block_on(async {
                let _ = tokio::time::timeout(Duration::from_secs(4), connection.close()).await;
            });
        }
    }
}
