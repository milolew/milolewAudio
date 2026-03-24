//! # ma-core
//!
//! Shared type definitions for communication between the audio engine and GUI
//! in milolew Audio DAW. This crate defines the "language" both sides speak:
//! commands, events, IDs, time units, and buffer types.
//!
//! **Dependency rule:** Both `ma-audio-engine` and `ma-ui` depend on this crate.
//! This crate NEVER depends on either of them.

pub mod audio_buffer;
pub mod commands;
pub mod device;
pub mod events;
pub mod ids;
pub mod midi_clip;
pub mod parameters;
pub mod time;

pub use audio_buffer::{AudioBuffer, BufferError};
pub use commands::{EngineCommand, TopologyCommand};
pub use events::EngineEvent;
pub use ids::{ClipId, NodeId, TrackId};
pub use midi_clip::{MidiClip, MidiClipRef};
pub use parameters::{
    ControllerNumber, MidiChannel, MidiNote, MidiRangeError, TrackConfig, TrackType,
    TransportState, Velocity,
};
pub use time::{FrameCount, SamplePos, Tick, TimeError, PPQN};
