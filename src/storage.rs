#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use crate::state::Preferences;

/// Errors returned when persisting preferences to storage.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("could not resolve preferences config path")]
    MissingConfigPath,
    #[error("failed to create preferences directory: {0}")]
    CreateDir(#[source] std::io::Error),
    #[error("failed to serialize preferences: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to write preferences file: {0}")]
    Write(#[source] std::io::Error),
}

/// Persistence abstraction for loading and saving [`Preferences`].
pub trait PreferencesStore {
    fn load(&self) -> Preferences;
    fn save(&self, prefs: &Preferences) -> Result<(), StorageError>;
}

/// JSON file-backed [`PreferencesStore`] implementation.
#[derive(Debug, Clone)]
pub struct JsonFileStore {
    path: Option<PathBuf>,
}

impl JsonFileStore {
    /// Creates a store targeting `~/Library/Application Support/Onda/volumes.json`.
    pub fn new() -> Self {
        let path = std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Onda")
                .join("volumes.json")
        });
        Self { path }
    }

    /// Creates a store targeting an explicit file path (useful for testing).
    pub fn with_path(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }
}

impl Default for JsonFileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PreferencesStore for JsonFileStore {
    fn load(&self) -> Preferences {
        let Some(ref path) = self.path else {
            return Preferences::default();
        };
        let Ok(contents) = fs::read_to_string(path) else {
            return Preferences::default();
        };
        serde_json::from_str(&contents).unwrap_or_default()
    }

    fn save(&self, prefs: &Preferences) -> Result<(), StorageError> {
        let path = self.path.as_ref().ok_or(StorageError::MissingConfigPath)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StorageError::CreateDir)?;
        }
        let json = serde_json::to_string_pretty(prefs)?;
        fs::write(path, json).map_err(StorageError::Write)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ThemeMode;

    #[test]
    fn json_file_store_should_return_defaults_when_file_missing() {
        let path = std::env::temp_dir().join("onda_missing_prefs_test.json");
        let _ = fs::remove_file(&path);
        let store = JsonFileStore::with_path(path);
        let prefs = store.load();
        assert_eq!(prefs.max_volume_limit, 2.0);
    }

    #[test]
    fn json_file_store_should_return_defaults_when_json_invalid() {
        let path = std::env::temp_dir().join("onda_invalid_prefs_test.json");
        fs::write(&path, "{ not valid json").expect("write invalid json");
        let store = JsonFileStore::with_path(path.clone());
        let prefs = store.load();
        let _ = fs::remove_file(&path);
        assert_eq!(prefs.theme, ThemeMode::Auto);
        assert_eq!(prefs.max_volume_limit, 2.0);
    }

    #[test]
    fn json_file_store_should_roundtrip_preferences() {
        let path = std::env::temp_dir().join("onda_roundtrip_prefs_test.json");
        let store = JsonFileStore::with_path(path.clone());
        let prefs = Preferences {
            theme: ThemeMode::Dark,
            max_volume_limit: 1.5,
            ..Default::default()
        };
        store.save(&prefs).expect("save prefs");
        let loaded = store.load();
        let _ = fs::remove_file(&path);
        assert_eq!(loaded.theme, ThemeMode::Dark);
        assert_eq!(loaded.max_volume_limit, 1.5);
    }

    #[test]
    fn json_file_store_without_path_should_error_on_save() {
        let store = JsonFileStore { path: None };
        let err = store
            .save(&Preferences::default())
            .expect_err("expected missing config path error");
        assert!(matches!(err, StorageError::MissingConfigPath));
        assert_eq!(err.to_string(), "could not resolve preferences config path");
    }
}
