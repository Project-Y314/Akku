use std::path::{Path, PathBuf};

use crate::model::Settings;

/// File-backed settings. Writes are atomic (temporary file + rename) so a crash
/// during save never leaves a truncated TOML file.
#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Settings {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|source| toml::from_str(&source).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let source = toml::to_string_pretty(settings)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        let temp = self.path.with_extension("toml.tmp");
        std::fs::write(&temp, source)?;
        std::fs::rename(temp, &self.path)?;
        Ok(())
    }
}

pub fn default_settings_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Akku")
        .join("settings.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_settings() {
        let dir = std::env::temp_dir().join(format!("akku-settings-{}", std::process::id()));
        let store = SettingsStore::new(dir.join("settings.toml"));
        let settings = Settings {
            theme: crate::model::Theme::White,
            poll_interval_secs: 30,
            preferred_refresh_hz: Some(144),
            autostart: true,
        };
        store.save(&settings).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.theme, crate::model::Theme::White);
        assert_eq!(loaded.poll_interval_secs, 30);
        assert_eq!(loaded.preferred_refresh_hz, Some(144));
        assert!(loaded.autostart);
        std::fs::remove_dir_all(dir).ok();
    }
}
