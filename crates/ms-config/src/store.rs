use crate::layout::ScreenLayout;
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigFile {
    pub settings: Settings,
    pub layout: ScreenLayout,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self { settings: Settings::default(), layout: ScreenLayout::default() }
    }
}

/// Loads and atomically persists `ConfigFile` at a given path. Public
/// device-visible state only (settings + layout); the trust store
/// (`ms_security::TrustStore`) and device identity are handled separately
/// since they have different security/persistence needs — see
/// `secure_storage.rs` and the crate-level docs.
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the platform-appropriate config directory
    /// (`%APPDATA%\MouseShare` on Windows, `~/Library/Application
    /// Support/MouseShare` on macOS) joined with `config.toml`. Callers on
    /// Linux (e.g. this crate's own CI) get XDG's equivalent from the same
    /// `directories` lookup.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "MouseShare", "MouseShare")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn load(&self) -> Result<ConfigFile, ConfigError> {
        if !self.path.exists() {
            return Ok(ConfigFile::default());
        }
        let contents = std::fs::read_to_string(&self.path)?;
        Ok(toml::from_str(&contents)?)
    }

    /// Writes via a temp file + rename in the same directory so a crash or
    /// power loss mid-write can never leave a truncated/corrupt config
    /// behind — the daemon reloads this on every restart, so a corrupt
    /// file would otherwise mean losing every pairing and layout setting.
    pub fn save(&self, config: &ConfigFile) -> Result<(), ConfigError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = toml::to_string_pretty(config)?;

        let tmp_path = tmp_path_for(&self.path);
        {
            let mut f = std::fs::File::create(&tmp_path)?;
            f.write_all(serialized.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_as_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("config.toml"));
        assert_eq!(store.load().unwrap(), ConfigFile::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("nested").join("config.toml"));

        let mut config = ConfigFile::default();
        config.settings.clipboard_sharing_enabled = true;
        config.settings.language = "fr-FR".to_string();
        store.save(&config).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn save_does_not_leave_a_temp_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let store = ConfigStore::new(&path);
        store.save(&ConfigFile::default()).unwrap();

        assert!(path.exists());
        assert!(!tmp_path_for(&path).exists());
    }
}
