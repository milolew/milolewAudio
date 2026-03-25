//! AudioDeviceManager — manages cpal host, device enumeration, and stream lifecycle.
//!
//! This struct lives on the UI thread (cpal::Stream is !Send on some platforms).
//! It creates and destroys cpal streams, wiring them to the audio engine's
//! `CallbackState` and `InputCaptureState`.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

use ma_core::device::{AudioDeviceConfig, AudioDeviceInfo, DeviceEnumeration, DeviceStatus};

use crate::callback::{audio_callback, CallbackState};
use crate::engine::{build_engine, EngineConfig, EngineHandle};
use crate::input_capture::{create_input_capture, InputCaptureState};

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("No output device available")]
    NoOutputDevice,

    #[error("No input device available")]
    NoInputDevice,

    #[error("Device '{0}' not found")]
    DeviceNotFound(String),

    #[error("Unsupported stream config: {0}")]
    UnsupportedConfig(String),

    #[error("Failed to build output stream: {0}")]
    OutputStreamError(String),

    #[error("Failed to build input stream: {0}")]
    InputStreamError(String),

    #[error("Failed to start stream: {0}")]
    PlayError(String),

    #[error("Audio graph topology error: {0}")]
    TopologyError(#[from] crate::graph::topology::TopologyError),
}

/// Manages cpal device and stream lifecycle.
///
/// Must be created and used on the UI/main thread.
pub struct AudioDeviceManager {
    host: cpal::Host,
    output_stream: Option<cpal::Stream>,
    input_stream: Option<cpal::Stream>,
    current_config: AudioDeviceConfig,
    current_status: DeviceStatus,
    enumeration: DeviceEnumeration,
}

impl Default for AudioDeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioDeviceManager {
    /// Create a new device manager using the default cpal host.
    pub fn new() -> Self {
        let host = cpal::default_host();
        Self {
            host,
            output_stream: None,
            input_stream: None,
            current_config: AudioDeviceConfig::default(),
            current_status: DeviceStatus::Offline {
                reason: "Not started".into(),
            },
            enumeration: DeviceEnumeration::default(),
        }
    }

    /// Enumerate all available audio devices.
    pub fn enumerate_devices(&mut self) -> &DeviceEnumeration {
        let mut output_devices = Vec::new();
        let mut input_devices = Vec::new();

        let default_output_name = self
            .host
            .default_output_device()
            .and_then(|d| d.name().ok());
        let default_input_name = self.host.default_input_device().and_then(|d| d.name().ok());

        if let Ok(devices) = self.host.output_devices() {
            for device in devices {
                if let Some(info) = device_to_info(&device, default_output_name.as_deref()) {
                    output_devices.push(info);
                }
            }
        }

        if let Ok(devices) = self.host.input_devices() {
            for device in devices {
                if let Some(info) = device_to_info(&device, default_input_name.as_deref()) {
                    input_devices.push(info);
                }
            }
        }

        self.enumeration = DeviceEnumeration {
            output_devices,
            input_devices,
        };
        &self.enumeration
    }

    /// Apply a device configuration: stop existing streams, rebuild engine, start new streams.
    ///
    /// Returns a new `EngineHandle` for the UI to communicate with the audio engine.
    /// This causes a brief audio gap (~50-200ms), acceptable for settings changes.
    pub fn apply_config(
        &mut self,
        config: AudioDeviceConfig,
        engine_config: EngineConfig,
    ) -> Result<EngineHandle, DeviceError> {
        // 1. Stop existing streams
        self.stop();
        self.current_status = DeviceStatus::Switching;

        // 2. Resolve output device and query its channel count
        let output_device = self.resolve_output_device(&config)?;
        let output_device_name = output_device.name().unwrap_or_else(|_| "Unknown".into());
        let output_channels = output_device
            .default_output_config()
            .map(|c| c.channels())
            .unwrap_or(2)
            .min(2); // Clamp to stereo max for now (engine is stereo)

        // 3. Build stream config
        let output_stream_config = cpal::StreamConfig {
            channels: output_channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(config.buffer_size),
        };

        // 4. Build the engine
        let (mut callback_state, engine_handle) = build_engine(engine_config)?;

        // 5. Set up input capture if enabled
        let mut input_device_name = None;
        if config.input_enabled {
            if let Ok(input_device) = self.resolve_input_device(&config) {
                let dev_name = input_device.name().unwrap_or_else(|_| "Unknown".into());
                let input_channels = input_device
                    .default_input_config()
                    .map(|c| c.channels())
                    .unwrap_or(2)
                    .min(2); // Clamp to stereo max

                let input_stream_config = cpal::StreamConfig {
                    channels: input_channels,
                    sample_rate: cpal::SampleRate(config.sample_rate),
                    buffer_size: cpal::BufferSize::Fixed(config.buffer_size),
                };

                let (capture_state, capture_reader) =
                    create_input_capture(input_channels as usize, config.buffer_size);
                callback_state.input_capture_reader = Some(capture_reader);

                let error_flag = std::sync::Arc::clone(&callback_state.device_error_flag);
                match self.build_input_stream(
                    input_device,
                    input_stream_config,
                    capture_state,
                    error_flag,
                ) {
                    Ok(stream) => {
                        self.input_stream = Some(stream);
                        input_device_name = Some(dev_name);
                    }
                    Err(e) => {
                        log::warn!("Failed to create input stream: {e}. Continuing without input.");
                    }
                }
            }
        }

        // 6. Build output stream (moves callback_state into closure)
        let output_stream =
            self.build_output_stream(output_device, output_stream_config, callback_state)?;
        self.output_stream = Some(output_stream);

        // 7. Start streams
        if let Some(ref stream) = self.output_stream {
            stream
                .play()
                .map_err(|e| DeviceError::PlayError(e.to_string()))?;
        }
        if let Some(ref stream) = self.input_stream {
            if let Err(e) = stream.play() {
                log::warn!("Failed to start input stream: {e}");
            }
        }

        // 8. Update state
        self.current_config = config.clone();
        self.current_status = DeviceStatus::Active {
            output_device: output_device_name,
            input_device: input_device_name,
            actual_sample_rate: config.sample_rate,
            actual_buffer_size: config.buffer_size,
        };

        Ok(engine_handle)
    }

    /// Stop all streams and release resources.
    pub fn stop(&mut self) {
        // Dropping streams blocks until current callback finishes — safe
        self.input_stream = None;
        self.output_stream = None;
        self.current_status = DeviceStatus::Offline {
            reason: "Stopped".into(),
        };
    }

    /// Current device status.
    pub fn status(&self) -> &DeviceStatus {
        &self.current_status
    }

    /// Current device configuration.
    pub fn current_config(&self) -> &AudioDeviceConfig {
        &self.current_config
    }

    /// Last enumerated devices.
    pub fn available_devices(&self) -> &DeviceEnumeration {
        &self.enumeration
    }

    // --- Private helpers ---

    fn resolve_output_device(
        &self,
        config: &AudioDeviceConfig,
    ) -> Result<cpal::Device, DeviceError> {
        if let Some(ref name) = config.output_device_name {
            self.find_device_by_name(name, true)
                .ok_or_else(|| DeviceError::DeviceNotFound(name.clone()))
        } else {
            self.host
                .default_output_device()
                .ok_or(DeviceError::NoOutputDevice)
        }
    }

    fn resolve_input_device(
        &self,
        config: &AudioDeviceConfig,
    ) -> Result<cpal::Device, DeviceError> {
        if let Some(ref name) = config.input_device_name {
            self.find_device_by_name(name, false)
                .ok_or_else(|| DeviceError::DeviceNotFound(name.clone()))
        } else {
            self.host
                .default_input_device()
                .ok_or(DeviceError::NoInputDevice)
        }
    }

    fn find_device_by_name(&self, name: &str, output: bool) -> Option<cpal::Device> {
        let devices = if output {
            self.host.output_devices().ok()?
        } else {
            self.host.input_devices().ok()?
        };

        devices
            .into_iter()
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
    }

    fn build_output_stream(
        &self,
        device: cpal::Device,
        config: cpal::StreamConfig,
        mut callback_state: CallbackState,
    ) -> Result<cpal::Stream, DeviceError> {
        let num_channels = config.channels as usize;
        let error_flag = std::sync::Arc::clone(&callback_state.device_error_flag);
        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let num_frames = (output.len() / num_channels) as u32;
                    audio_callback(&mut callback_state, output, num_frames);
                },
                move |err| {
                    log::error!("Output stream error: {err}");
                    // Signal error to audio callback via atomic flag.
                    // Mapping: 1=Overflow, 2=Underflow, 3=DeviceLost, 4=Unknown
                    let code = cpal_error_to_code(&err);
                    error_flag.store(code, std::sync::atomic::Ordering::Relaxed);
                },
                None,
            )
            .map_err(|e| DeviceError::OutputStreamError(e.to_string()))?;

        Ok(stream)
    }

    fn build_input_stream(
        &self,
        device: cpal::Device,
        config: cpal::StreamConfig,
        mut capture_state: InputCaptureState,
        device_error_flag: std::sync::Arc<std::sync::atomic::AtomicU8>,
    ) -> Result<cpal::Stream, DeviceError> {
        let stream = device
            .build_input_stream(
                &config,
                move |input: &[f32], _: &cpal::InputCallbackInfo| {
                    capture_state.capture(input);
                },
                move |err| {
                    log::error!("Input stream error: {err}");
                    let code = cpal_error_to_code(&err);
                    device_error_flag.store(code, std::sync::atomic::Ordering::Relaxed);
                },
                None,
            )
            .map_err(|e| DeviceError::InputStreamError(e.to_string()))?;

        Ok(stream)
    }
}

/// Map a cpal stream error to a numeric code for the atomic flag.
/// 1=Overflow, 2=Underflow, 3=DeviceLost, 4=Unknown
fn cpal_error_to_code(err: &cpal::StreamError) -> u8 {
    match err {
        cpal::StreamError::DeviceNotAvailable => 3,
        cpal::StreamError::BackendSpecific { .. } => 4,
    }
}

/// Convert a cpal Device to AudioDeviceInfo.
fn device_to_info(device: &cpal::Device, default_name: Option<&str>) -> Option<AudioDeviceInfo> {
    let name = device.name().ok()?;
    let is_default = default_name.map(|d| d == name).unwrap_or(false);

    // Query supported configs to extract sample rates and buffer sizes
    let mut sample_rates = Vec::new();
    let mut min_buffer = u32::MAX;
    let mut max_buffer = 0u32;
    let mut max_channels = 0u16;

    // Collect config ranges from either output or input configs
    let config_ranges: Vec<_> = device
        .supported_output_configs()
        .map(|c| c.collect::<Vec<_>>())
        .or_else(|_| {
            device
                .supported_input_configs()
                .map(|c| c.collect::<Vec<_>>())
        })
        .ok()?;

    for config_range in config_ranges {
        let min_rate = config_range.min_sample_rate().0;
        let max_rate = config_range.max_sample_rate().0;

        // Add common sample rates within supported range
        for &rate in &[44100, 48000, 88200, 96000, 176400, 192000] {
            if rate >= min_rate && rate <= max_rate && !sample_rates.contains(&rate) {
                sample_rates.push(rate);
            }
        }

        let channels = config_range.channels();
        if channels > max_channels {
            max_channels = channels;
        }

        if let cpal::SupportedBufferSize::Range { min, max } = config_range.buffer_size() {
            if *min < min_buffer {
                min_buffer = *min;
            }
            if *max > max_buffer {
                max_buffer = *max;
            }
        }
    }

    // Fallback for buffer sizes if not reported
    if min_buffer == u32::MAX {
        min_buffer = 64;
    }
    if max_buffer == 0 {
        max_buffer = 4096;
    }

    sample_rates.sort_unstable();

    Some(AudioDeviceInfo {
        name,
        is_default,
        supported_sample_rates: sample_rates,
        min_buffer_size: min_buffer,
        max_buffer_size: max_buffer,
        max_channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_manager_enumerates_devices() {
        // This test runs on any platform — cpal always provides at least
        // a host (even if no physical devices are available).
        let mut dm = AudioDeviceManager::new();
        let enumeration = dm.enumerate_devices();
        // We can't assert specific devices exist in CI, but the enumeration
        // should complete without panic and return a valid struct.
        // Enumeration should complete without panic — no specific devices guaranteed in CI.
        let _ = &enumeration;
    }

    #[test]
    fn device_error_has_topology_variant() {
        let err =
            DeviceError::TopologyError(crate::graph::topology::TopologyError::CycleDetected {
                total: 3,
                sorted: 2,
                skipped: 1,
            });
        let msg = err.to_string();
        assert!(msg.contains("cycle detected"));
    }

    #[test]
    fn cpal_error_code_mapping() {
        // DeviceNotAvailable maps to code 3 (DeviceLost)
        let code = cpal_error_to_code(&cpal::StreamError::DeviceNotAvailable);
        assert_eq!(code, 3);
    }
}
