//! Parameter types and transport state shared between engine and UI.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error returned when a MIDI value is out of its valid range.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MidiRangeError {
    /// Channel value must be 0-15.
    #[error("MIDI channel {value} out of range (valid: 0-15)")]
    ChannelOutOfRange { value: u8 },

    /// Note value must be 0-127.
    #[error("MIDI note {value} out of range (valid: 0-127)")]
    NoteOutOfRange { value: u8 },

    /// Velocity value must be 0-127.
    #[error("MIDI velocity {value} out of range (valid: 0-127)")]
    VelocityOutOfRange { value: u8 },

    /// Controller number must be 0-119.
    #[error("MIDI controller number {value} out of range (valid: 0-119)")]
    ControllerOutOfRange { value: u8 },
}

/// Validated MIDI channel (0-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiChannel(u8);

impl MidiChannel {
    /// The maximum valid MIDI channel value.
    pub const MAX: u8 = 15;

    /// Get the inner u8 value.
    #[inline]
    pub fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for MidiChannel {
    type Error = MidiRangeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > Self::MAX {
            Err(MidiRangeError::ChannelOutOfRange { value })
        } else {
            Ok(Self(value))
        }
    }
}

/// Validated MIDI note number (0-127).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MidiNote(u8);

impl MidiNote {
    /// The maximum valid MIDI note value.
    pub const MAX: u8 = 127;

    /// Get the inner u8 value.
    #[inline]
    pub fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for MidiNote {
    type Error = MidiRangeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > Self::MAX {
            Err(MidiRangeError::NoteOutOfRange { value })
        } else {
            Ok(Self(value))
        }
    }
}

/// Validated MIDI velocity (0-127).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Velocity(u8);

impl Velocity {
    /// The maximum valid velocity value.
    pub const MAX: u8 = 127;

    /// Get the inner u8 value.
    #[inline]
    pub fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Velocity {
    type Error = MidiRangeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > Self::MAX {
            Err(MidiRangeError::VelocityOutOfRange { value })
        } else {
            Ok(Self(value))
        }
    }
}

/// Validated MIDI controller number (0-119).
///
/// Controllers 120-127 are reserved for channel mode messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ControllerNumber(u8);

impl ControllerNumber {
    /// The maximum valid controller number (119).
    /// Controllers 120-127 are channel mode messages.
    pub const MAX: u8 = 119;

    /// Get the inner u8 value.
    #[inline]
    pub fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for ControllerNumber {
    type Error = MidiRangeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > Self::MAX {
            Err(MidiRangeError::ControllerOutOfRange { value })
        } else {
            Ok(Self(value))
        }
    }
}

/// Whether a track plays audio files or MIDI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackType {
    /// Audio track with WavPlayerNode for clip playback.
    #[default]
    Audio,
    /// MIDI track with MidiPlayerNode and built-in synth.
    Midi,
}

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

    /// Track type: audio or MIDI.
    pub track_type: TrackType,
}

impl Default for TrackConfig {
    fn default() -> Self {
        Self {
            name: String::from("New Track"),
            channel_count: 2,
            input_enabled: false,
            initial_volume: 1.0,
            initial_pan: 0.0,
            track_type: TrackType::Audio,
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
    /// Pre-roll count-in: metronome plays but recording has not started yet.
    CountingIn,
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

impl MidiMessage {
    /// Create a validated NoteOn message.
    pub fn note_on(channel: MidiChannel, note: MidiNote, velocity: Velocity) -> Self {
        Self::NoteOn {
            channel: channel.value(),
            note: note.value(),
            velocity: velocity.value(),
        }
    }

    /// Create a validated NoteOff message.
    pub fn note_off(channel: MidiChannel, note: MidiNote, velocity: Velocity) -> Self {
        Self::NoteOff {
            channel: channel.value(),
            note: note.value(),
            velocity: velocity.value(),
        }
    }

    /// Create a validated ControlChange message.
    pub fn control_change(channel: MidiChannel, controller: ControllerNumber, value: u8) -> Self {
        Self::ControlChange {
            channel: channel.value(),
            controller: controller.value(),
            value: value.min(127),
        }
    }

    /// Create a validated PitchBend message.
    pub fn pitch_bend(channel: MidiChannel, value: i16) -> Self {
        Self::PitchBend {
            channel: channel.value(),
            value,
        }
    }

    /// Create a validated ProgramChange message.
    pub fn program_change(channel: MidiChannel, program: u8) -> Self {
        Self::ProgramChange {
            channel: channel.value(),
            program: program.min(127),
        }
    }
}

/// A MIDI event with tick-accurate timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiEvent {
    pub tick: crate::time::Tick,
    pub message: MidiMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_channel_valid_range() {
        for i in 0..=15 {
            assert!(MidiChannel::try_from(i).is_ok());
        }
        assert!(MidiChannel::try_from(16).is_err());
        assert!(MidiChannel::try_from(255).is_err());
    }

    #[test]
    fn midi_note_valid_range() {
        for i in 0..=127 {
            assert!(MidiNote::try_from(i).is_ok());
        }
        assert!(MidiNote::try_from(128).is_err());
        assert!(MidiNote::try_from(255).is_err());
    }

    #[test]
    fn velocity_valid_range() {
        for i in 0..=127 {
            assert!(Velocity::try_from(i).is_ok());
        }
        assert!(Velocity::try_from(128).is_err());
        assert!(Velocity::try_from(255).is_err());
    }

    #[test]
    fn controller_number_valid_range() {
        for i in 0..=119 {
            assert!(ControllerNumber::try_from(i).is_ok());
        }
        assert!(ControllerNumber::try_from(120).is_err());
        assert!(ControllerNumber::try_from(127).is_err());
        assert!(ControllerNumber::try_from(255).is_err());
    }

    #[test]
    fn midi_channel_value_roundtrip() {
        let ch = MidiChannel::try_from(10).unwrap();
        assert_eq!(ch.value(), 10);
    }

    #[test]
    fn midi_note_value_roundtrip() {
        let note = MidiNote::try_from(60).unwrap();
        assert_eq!(note.value(), 60);
    }

    #[test]
    fn velocity_value_roundtrip() {
        let vel = Velocity::try_from(100).unwrap();
        assert_eq!(vel.value(), 100);
    }

    #[test]
    fn controller_value_roundtrip() {
        let ctrl = ControllerNumber::try_from(64).unwrap();
        assert_eq!(ctrl.value(), 64);
    }

    #[test]
    fn validated_note_on_constructor() {
        let ch = MidiChannel::try_from(0).unwrap();
        let note = MidiNote::try_from(60).unwrap();
        let vel = Velocity::try_from(100).unwrap();
        let msg = MidiMessage::note_on(ch, note, vel);
        assert_eq!(
            msg,
            MidiMessage::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100
            }
        );
    }

    #[test]
    fn validated_control_change_constructor() {
        let ch = MidiChannel::try_from(5).unwrap();
        let ctrl = ControllerNumber::try_from(7).unwrap();
        let msg = MidiMessage::control_change(ch, ctrl, 80);
        assert_eq!(
            msg,
            MidiMessage::ControlChange {
                channel: 5,
                controller: 7,
                value: 80
            }
        );
    }

    #[test]
    fn error_messages_are_descriptive() {
        let err = MidiChannel::try_from(16).unwrap_err();
        assert_eq!(
            err.to_string(),
            "MIDI channel 16 out of range (valid: 0-15)"
        );

        let err = MidiNote::try_from(128).unwrap_err();
        assert_eq!(err.to_string(), "MIDI note 128 out of range (valid: 0-127)");

        let err = Velocity::try_from(200).unwrap_err();
        assert_eq!(
            err.to_string(),
            "MIDI velocity 200 out of range (valid: 0-127)"
        );

        let err = ControllerNumber::try_from(120).unwrap_err();
        assert_eq!(
            err.to_string(),
            "MIDI controller number 120 out of range (valid: 0-119)"
        );
    }

    #[test]
    fn transport_state_default_is_stopped() {
        assert_eq!(TransportState::default(), TransportState::Stopped);
    }

    #[test]
    fn track_config_default_values() {
        let config = TrackConfig::default();
        assert_eq!(config.name, "New Track");
        assert_eq!(config.channel_count, 2);
        assert!(!config.input_enabled);
        assert!((config.initial_volume - 1.0).abs() < f32::EPSILON);
        assert!(config.initial_pan.abs() < f32::EPSILON);
    }
}
