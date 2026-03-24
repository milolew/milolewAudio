//! Events sent from the audio engine to the UI via SPSC ring buffer.
//!
//! These are lightweight status updates: meter levels, transport position,
//! error conditions. The UI thread reads them each frame (~60fps) to
//! update the display.
//!
//! Events are ephemeral — if the ring buffer is full, the audio thread
//! drops new events (metering data is expendable). This is safe because
//! the next callback will send fresh data anyway.

use crate::ids::TrackId;
use crate::parameters::TransportState;
use crate::time::SamplePos;

/// An event from the audio engine to the UI.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    // ── Metering (sent every callback, ~every 5ms) ────────────
    /// Peak levels for a single track (post-fader).
    PeakMeter {
        track_id: TrackId,
        left: f32,
        right: f32,
    },

    /// Peak levels for the master bus.
    MasterPeakMeter { left: f32, right: f32 },

    /// Audio thread CPU load as a fraction (0.0–1.0).
    /// Computed as: actual_processing_time / available_budget_time.
    CpuLoad(f32),

    // ── Transport state ────────────────────────────────────────
    /// Current playhead position in samples.
    PlayheadPosition(SamplePos),

    /// Transport state has changed.
    TransportStateChanged(TransportState),

    // ── Recording ──────────────────────────────────────────────
    /// The record ring buffer for a track overflowed — samples were lost.
    RecordingOverflow { track_id: TrackId },

    /// Recording has been finalized for a track.
    RecordingComplete { track_id: TrackId },

    // ── Errors (non-fatal, engine keeps running) ───────────────
    /// Audio callback did not complete in time (buffer underrun).
    AudioUnderrun,

    /// An audio device error occurred.
    DeviceError(DeviceErrorKind),
}

/// Categories of device errors that the engine can report.
#[derive(Debug, Clone)]
pub enum DeviceErrorKind {
    /// The audio device was disconnected.
    DeviceDisconnected,

    /// The audio stream encountered an error.
    StreamError(StreamErrorCode),

    /// The requested sample rate is not supported.
    UnsupportedSampleRate(u32),

    /// The requested buffer size is not supported.
    UnsupportedBufferSize(u32),
}

/// Stream error codes — fixed-size enum, safe for lock-free ring buffers.
/// No heap allocation (replaces the previous `String` variant which could
/// trigger deallocation on the audio thread).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamErrorCode {
    Overflow,
    Underflow,
    DeviceLost,
    Unknown,
}
