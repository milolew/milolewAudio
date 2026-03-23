//! Mixer state — per-track meter peaks.

use std::collections::HashMap;

use crate::types::track::TrackId;

/// Per-track meter reading.
#[derive(Debug, Clone, Copy, Default)]
pub struct MeterPeaks {
    pub peak_l: f32,
    pub peak_r: f32,
}

#[derive(Debug, Clone, Default)]
pub struct MixerState {
    /// Peak meter values per track, updated from engine responses.
    pub meters: HashMap<TrackId, MeterPeaks>,
    /// CPU load from audio engine (0.0 - 1.0).
    pub cpu_load: f32,
}

impl MixerState {
    pub fn update_meter(&mut self, track_id: TrackId, peak_l: f32, peak_r: f32) {
        self.meters.insert(track_id, MeterPeaks { peak_l, peak_r });
    }

    pub fn get_meter(&self, track_id: TrackId) -> MeterPeaks {
        self.meters.get(&track_id).copied().unwrap_or_default()
    }
}
