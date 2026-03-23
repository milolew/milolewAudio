//! Parameter types and transport state shared between engine and UI.

use serde::{Deserialize, Serialize};

/// Configuration for creating a new track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackConfig {
    /// Human-readable track name (only used off-RT thread for display).
    pub name: String,

    /// Number of audio channels (1 = mono, 2 = stereo).
    pub channel_count: usize,

    /// Whether this track should receive audio input (for recording).
    pub input_enabled: bool,

    /// Initial volume (linear gain, default 1.0).
    pub initial_volume: f32,

    /// Initial pan (-1.0 left, 0.0 center, 1.0 right).
    pub initial_pan: f32,
}

impl Default for TrackConfig {
    fn default() -> Self {
        Self {
            name: String::from("New Track"),
            channel_count: 2,
            input_enabled: false,
            initial_volume: 1.0,
            initial_pan: 0.0,
        }
    }
}

/// Transport state machine.
///
/// ```text
///  Stopped ──Play──▶ Playing ──Stop──▶ Stopped
///     │                  │                ▲
///     │                  ├──Pause──▶ Paused ─┘ (Play resumes)
///     │                  │
///     │                  └──Record──▶ Recording ──Stop──▶ Stopped
///     │
///     └──Record+Play──▶ Recording ──Stop──▶ Stopped
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportState {
    #[default]
    Stopped,
    Playing,
    Paused,
    Recording,
}

/// MIDI-related types shared between engine and GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
}

/// A MIDI event with tick-accurate timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiEvent {
    pub tick: crate::time::Tick,
    pub message: MidiMessage,
}
