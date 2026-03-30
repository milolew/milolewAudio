//! Transport state — playhead position, tempo, play/record status.

use crate::types::time::{Tick, TimeSignature};

#[derive(Debug, Clone)]
pub struct TransportState {
    pub position: Tick,
    pub is_playing: bool,
    pub is_recording: bool,
    pub tempo: f64,
    pub time_signature: TimeSignature,
    pub loop_enabled: bool,
    pub loop_start: Tick,
    pub loop_end: Tick,
    pub metronome_enabled: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            position: 0,
            is_playing: false,
            is_recording: false,
            tempo: 120.0,
            time_signature: TimeSignature::default(),
            loop_enabled: false,
            loop_start: 0,
            loop_end: 7680, // 2 bars at 4/4
            metronome_enabled: false,
        }
    }
}
