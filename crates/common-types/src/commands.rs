//! Commands sent from the UI thread to the audio engine via SPSC ring buffer.
//!
//! **Thread safety:** All variants must be `Send`. No variant may contain types
//! that allocate on drop (the audio thread drains these commands and must not
//! trigger deallocation). For heap-allocated data (like audio clip data), use
//! `Arc<[f32]>` — the audio thread will hold a reference but never drop the last one.

use std::sync::Arc;

use crate::ids::{ClipId, TrackId};
use crate::parameters::TrackConfig;
use crate::time::SamplePos;

/// A command from the UI to the audio engine.
///
/// Commands are placed into an SPSC ring buffer (rtrb) by the UI thread
/// and drained at the beginning of each audio callback.
///
/// Design constraints:
/// - Must be `Send` (crosses thread boundary)
/// - Should be small (fits in ring buffer slot efficiently)
/// - No types that allocate on drop in audio-thread-critical variants
#[derive(Debug)]
pub enum EngineCommand {
    // ── Transport ──────────────────────────────────────────────
    /// Start playback from current position.
    Play,

    /// Stop playback and reset position to last play-start point.
    Stop,

    /// Pause playback at current position (resume with Play).
    Pause,

    /// Seek to an absolute sample position.
    SetPosition(SamplePos),

    /// Change the tempo (BPM).
    SetTempo(f64),

    /// Configure loop region.
    SetLoop {
        start: SamplePos,
        end: SamplePos,
        enabled: bool,
    },

    // ── Track control ──────────────────────────────────────────
    /// Set track volume (linear gain, 0.0–1.0+).
    SetTrackVolume { track_id: TrackId, volume: f32 },

    /// Set track pan (-1.0 = full left, 0.0 = center, 1.0 = full right).
    SetTrackPan { track_id: TrackId, pan: f32 },

    /// Mute or unmute a track.
    SetTrackMute { track_id: TrackId, mute: bool },

    /// Solo or unsolo a track.
    SetTrackSolo { track_id: TrackId, solo: bool },

    // ── Recording ──────────────────────────────────────────────
    /// Arm or disarm a track for recording.
    ArmTrack { track_id: TrackId, armed: bool },

    /// Begin recording on all armed tracks (transport must be playing).
    StartRecording,

    /// Stop recording on all armed tracks.
    StopRecording,

    // ── Graph topology ─────────────────────────────────────────
    /// Add a new track to the audio graph.
    AddTrack {
        track_id: TrackId,
        config: TrackConfig,
    },

    /// Remove a track from the audio graph.
    RemoveTrack { track_id: TrackId },

    /// Load a pre-decoded audio clip for playback.
    /// `data` is an `Arc<[f32]>` (non-interleaved) so the audio thread only reads it
    /// and never drops the last reference (the project state on the UI side holds another Arc).
    LoadClip {
        track_id: TrackId,
        clip_id: ClipId,
        data: Arc<[f32]>,
        channels: usize,
        start_sample: SamplePos,
        length_samples: SamplePos,
    },

    /// Remove a clip from a track.
    RemoveClip {
        track_id: TrackId,
        clip_id: ClipId,
    },

    // ── Engine lifecycle ───────────────────────────────────────
    /// Gracefully shut down the audio engine.
    Shutdown,
}

// Ensure EngineCommand is Send (required for cross-thread ring buffer).
// This is a compile-time check — if any variant contains !Send data, it fails.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<EngineCommand>();
};
