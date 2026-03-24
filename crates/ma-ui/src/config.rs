//! Preferences persistence — load/save audio device config to JSON.

use std::path::PathBuf;

use ma_core::device::AudioDeviceConfig;
use serde::{Deserialize, Serialize};

const CONFIG_DIR_NAME: &str = "milolew-audio";
const CONFIG_FILE_NAME: &str = "preferences.json";

/// Persistent preferences (saved to disk).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Preferences {
    pub audio: AudioDeviceConfig,
}

/// Get the config file path (platform-specific).
/// Linux: ~/.config/milolew-audio/preferences.json
/// Windows: %APPDATA%/milolew-audio/preferences.json
pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_DIR_NAME)
        .join(CONFIG_FILE_NAME)
}

/// Load preferences from disk. Returns defaults if file doesn't exist or is invalid.
pub fn load_preferences() -> Preferences {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
            log::warn!("Invalid preferences file: {e}. Using defaults.");
            Preferences::default()
        }),
        Err(_) => Preferences::default(),
    }
}

/// Save preferences to disk. Errors are logged but not propagated.
pub fn save_preferences(prefs: &Preferences) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("Failed to create config directory: {e}");
            return;
        }
    }
    match serde_json::to_string_pretty(prefs) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Failed to write preferences: {e}");
            }
        }
        Err(e) => {
            log::warn!("Failed to serialize preferences: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preferences_round_trip() {
        let prefs = Preferences::default();
        let json = serde_json::to_string(&prefs).unwrap();
        let loaded: Preferences = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.audio.sample_rate, 48000);
        assert_eq!(loaded.audio.buffer_size, 256);
    }
}
