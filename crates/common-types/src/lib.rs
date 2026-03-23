//! # common-types
//!
//! Shared type definitions for communication between the audio engine and GUI
//! in milolew Audio DAW. This crate defines the "language" both sides speak:
//! commands, events, IDs, time units, and buffer types.
//!
//! **Dependency rule:** Both `audio-engine` and `gui-interface` depend on this crate.
//! This crate NEVER depends on either of them.

pub mod audio_buffer;
pub mod commands;
pub mod events;
pub mod ids;
pub mod parameters;
pub mod time;

pub use audio_buffer::AudioBuffer;
pub use commands::{EngineCommand, TopologyCommand};
pub use events::EngineEvent;
pub use ids::{ClipId, NodeId, TrackId};
pub use parameters::{TrackConfig, TransportState};
pub use time::{FrameCount, SamplePos, Tick, PPQN};
