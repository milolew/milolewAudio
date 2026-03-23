//! Audio device types shared between the audio engine and GUI.
//!
//! These types describe available audio hardware and user-selected
//! configuration without depending on cpal or any platform-specific crate.

use serde::{Deserialize, Serialize};

/// Information about a discovered audio device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    /// Human-readable device name (also used as identifier for persistence).
    pub name: String,
    /// Whether this is the system default device.
    pub is_default: bool,
    /// Supported sample rates (e.g., \[44100, 48000, 96000\]).
    pub supported_sample_rates: Vec<u32>,
    /// Minimum supported buffer size in frames.
    pub min_buffer_size: u32,
    /// Maximum supported buffer size in frames.
    pub max_buffer_size: u32,
    /// Maximum number of channels.
    pub max_channels: u16,
}

/// User-selected audio device configuration (persisted to preferences file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceConfig {
    /// Output device name. `None` means use system default.
    pub output_device_name: Option<String>,
    /// Input device name. `None` means use system default.
    pub input_device_name: Option<String>,
    /// Desired sample rate in Hz.
    pub sample_rate: u32,
    /// Desired buffer size in frames.
    pub buffer_size: u32,
    /// Whether input is enabled (recording).
    pub input_enabled: bool,
}

impl Default for AudioDeviceConfig {
    fn default() -> Self {
        Self {
            output_device_name: None,
            input_device_name: None,
            sample_rate: 48000,
            buffer_size: 256,
            input_enabled: true,
        }
    }
}

/// Status of the audio device subsystem.
#[derive(Debug, Clone)]
pub enum DeviceStatus {
    /// Audio streams are active and running.
    Active {
        output_device: String,
        input_device: Option<String>,
        actual_sample_rate: u32,
        actual_buffer_size: u32,
    },
    /// No audio device available or configured.
    Offline { reason: String },
    /// Device is being switched (transient state).
    Switching,
}

/// Snapshot of all available audio devices.
#[derive(Debug, Clone, Default)]
pub struct DeviceEnumeration {
    pub output_devices: Vec<AudioDeviceInfo>,
    pub input_devices: Vec<AudioDeviceInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sensible() {
        let config = AudioDeviceConfig::default();
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.buffer_size, 256);
        assert!(config.output_device_name.is_none());
        assert!(config.input_enabled);
    }

    #[test]
    fn config_round_trip_json() {
        let config = AudioDeviceConfig {
            output_device_name: Some("M-Audio Duo".into()),
            input_device_name: Some("M-Audio Duo".into()),
            sample_rate: 44100,
            buffer_size: 128,
            input_enabled: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AudioDeviceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sample_rate, 44100);
        assert_eq!(deserialized.buffer_size, 128);
        assert_eq!(deserialized.output_device_name, Some("M-Audio Duo".into()));
    }
}
