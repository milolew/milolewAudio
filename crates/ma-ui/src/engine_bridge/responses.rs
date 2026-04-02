//! Responses sent from the audio engine back to the UI via SPSC ring buffer.

use std::path::PathBuf;

use crate::types::time::Tick;
use crate::types::track::TrackId;

/// All responses the engine can send to the UI.
#[derive(Debug, Clone)]
pub enum EngineResponse {
    /// Transport state update (sent every ~16ms while playing).
    TransportUpdate {
        position: Tick,
        is_playing: bool,
        is_recording: bool,
    },

    /// Tempo changed (confirmation or external sync).
    TempoUpdate(f64),

    /// Peak meter levels for a track.
    MeterUpdate {
        track_id: TrackId,
        peak_l: f32,
        peak_r: f32,
    },

    /// Master bus peak meter levels.
    MasterMeterUpdate { peak_l: f32, peak_r: f32 },

    /// Audio thread CPU load (0.0 - 1.0).
    CpuLoad(f32),

    /// Recording finalized — WAV file written to disk.
    RecordingComplete {
        track_id: TrackId,
        path: PathBuf,
        total_samples: u64,
    },

    /// Recording error (disk I/O failure).
    RecordingError { track_id: TrackId, error: String },

    /// Count-in beat reached (for UI display).
    CountInBeat { bar: u8, beat: u8, total_bars: u8 },

    /// Count-in complete — recording has started.
    CountInComplete,
}
