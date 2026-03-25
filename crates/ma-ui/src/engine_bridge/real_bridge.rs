//! RealEngineBridge — wraps ma-audio-engine's EngineHandle for the UI.
//!
//! Translates between ma-core::EngineEvent and the UI's EngineResponse type,
//! and forwards UI commands as ma-core::EngineCommand.

use std::sync::atomic::Ordering;

use ma_audio_engine::engine::EngineHandle;
use ma_core::commands::{EngineCommand as CoreCommand, TopologyCommand};
use ma_core::events::EngineEvent;
use ma_core::parameters::TransportState;
use ma_core::time::PPQN;

use crate::engine_bridge::responses::EngineResponse;
use crate::types::track::TrackId;

/// Wraps the real audio engine handle for UI communication.
pub struct RealEngineBridge {
    handle: EngineHandle,
    /// Cache transport playing state for poll_responses.
    last_playing: bool,
    last_recording: bool,
    /// Current tempo for sample-to-tick conversion.
    tempo: f64,
    /// Sample rate for sample-to-tick conversion.
    sample_rate: f64,
}

impl RealEngineBridge {
    pub fn new(handle: EngineHandle) -> Self {
        let sample_rate = handle.config.sample_rate as f64;
        Self {
            handle,
            last_playing: false,
            last_recording: false,
            tempo: 120.0,
            sample_rate,
        }
    }

    /// Convert sample position to ticks using current tempo.
    /// ticks = samples * PPQN * tempo / (sample_rate * 60)
    fn samples_to_ticks(&self, samples: i64) -> i64 {
        if self.sample_rate == 0.0 {
            return 0;
        }
        (samples as f64 * PPQN as f64 * self.tempo / (self.sample_rate * 60.0)) as i64
    }

    /// Send a core engine command.
    pub fn send_command(&mut self, cmd: CoreCommand) -> bool {
        self.handle.command_producer.push(cmd).is_ok()
    }

    /// Poll all pending engine events and translate to UI responses.
    /// Writes into the caller-owned buffer to avoid per-frame allocation.
    ///
    /// Also reads atomic playhead position and recording state.
    pub fn poll_responses(&mut self, out: &mut Vec<EngineResponse>) {
        out.clear();

        // Drain all events from the engine
        while let Ok(event) = self.handle.event_consumer.pop() {
            match event {
                EngineEvent::CpuLoad(load) => {
                    out.push(EngineResponse::CpuLoad(load));
                }
                EngineEvent::PeakMeter {
                    track_id,
                    left,
                    right,
                } => {
                    out.push(EngineResponse::MeterUpdate {
                        track_id,
                        peak_l: left,
                        peak_r: right,
                    });
                }
                EngineEvent::MasterPeakMeter { left, right } => {
                    out.push(EngineResponse::MasterMeterUpdate {
                        peak_l: left,
                        peak_r: right,
                    });
                }
                EngineEvent::TransportStateChanged(state) => {
                    self.last_playing = state == TransportState::Playing;
                    self.last_recording = state == TransportState::Recording;
                }
                EngineEvent::PlayheadPosition(samples) => {
                    let ticks = self.samples_to_ticks(samples);
                    out.push(EngineResponse::TransportUpdate {
                        position: ticks,
                        is_playing: self.last_playing,
                        is_recording: self.last_recording,
                    });
                }
                _ => {}
            }
        }

        // Always send a transport update with current atomic playhead
        let playhead_samples = self.handle.playhead_position.load(Ordering::Relaxed);
        let is_recording = self.handle.is_recording.load(Ordering::Relaxed);

        out.push(EngineResponse::TransportUpdate {
            position: self.samples_to_ticks(playhead_samples),
            is_playing: self.last_playing,
            is_recording,
        });
    }

    /// Get track handles for reading atomic parameters.
    pub fn track_handles(&self) -> &[ma_audio_engine::engine::TrackHandle] {
        &self.handle.tracks
    }

    /// Map a UI TrackId to a core command for track volume.
    pub fn send_track_volume(&mut self, track_id: TrackId, volume: f32) {
        self.send_command(CoreCommand::SetTrackVolume { track_id, volume });
    }

    /// Map a UI TrackId to a core command for track pan.
    pub fn send_track_pan(&mut self, track_id: TrackId, pan: f32) {
        self.send_command(CoreCommand::SetTrackPan { track_id, pan });
    }

    /// Map a UI TrackId to a core command for track mute.
    pub fn send_track_mute(&mut self, track_id: TrackId, mute: bool) {
        self.send_command(CoreCommand::SetTrackMute { track_id, mute });
    }

    /// Map a UI TrackId to a core command for track solo.
    pub fn send_track_solo(&mut self, track_id: TrackId, solo: bool) {
        self.send_command(CoreCommand::SetTrackSolo { track_id, solo });
    }

    /// Send a topology command (heap-allocating, non-RT).
    pub fn send_topology_command(&self, cmd: TopologyCommand) -> bool {
        self.handle.topology_command_sender.send(cmd).is_ok()
    }

    /// Get the current sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.handle.config.sample_rate
    }

    /// Get the current tempo.
    pub fn tempo(&self) -> f64 {
        self.tempo
    }
}
