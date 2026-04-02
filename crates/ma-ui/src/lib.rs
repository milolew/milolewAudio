//! milolew Audio — GUI Interface
//!
//! DAW GUI built with vizia, featuring:
//! - Arrangement View (timeline with track lanes and clips)
//! - Mixer View (channel strips with faders, meters, mute/solo)
//! - Piano Roll (MIDI note editor with mouse-based drawing)
//!
//! Communicates with the audio engine via lock-free SPSC ring buffers.

pub mod app_data;
pub mod audio_preview;
pub mod config;
pub mod demo_data;
pub mod engine_bridge;
pub mod file_loader;
pub mod state;
pub mod types;
pub mod undo_actions;
pub mod views;
pub mod widgets;
