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

    // ── Count-in ──────────────────────────────────────────────
    /// A count-in beat has been reached (for UI display).
    CountInBeat { bar: u8, beat: u8, total_bars: u8 },

    /// Count-in is complete — recording has started.
    CountInComplete,

    // ── Errors (non-fatal, engine keeps running) ───────────────
    /// Audio callback did not complete in time (buffer underrun).
    AudioUnderrun,

    /// The audio thread panicked. Engine outputs silence until restarted.
    AudioThreadPanic,

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TrackId;
    use crate::parameters::TransportState;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn engine_event_is_send() {
        assert_send::<EngineEvent>();
    }

    #[test]
    fn engine_event_is_sync() {
        assert_sync::<EngineEvent>();
    }

    #[test]
    fn device_error_kind_is_send_and_sync() {
        assert_send::<DeviceErrorKind>();
        assert_sync::<DeviceErrorKind>();
    }

    #[test]
    fn stream_error_code_is_send_and_sync() {
        assert_send::<StreamErrorCode>();
        assert_sync::<StreamErrorCode>();
    }

    #[test]
    fn engine_event_metering_variants() {
        let track_id = TrackId::new();
        let events: Vec<EngineEvent> = vec![
            EngineEvent::PeakMeter {
                track_id,
                left: 0.5,
                right: 0.8,
            },
            EngineEvent::MasterPeakMeter {
                left: 0.9,
                right: 0.7,
            },
            EngineEvent::CpuLoad(0.45),
        ];
        assert_eq!(events.len(), 3);

        match &events[0] {
            EngineEvent::PeakMeter { left, right, .. } => {
                assert!((left - 0.5).abs() < f32::EPSILON);
                assert!((right - 0.8).abs() < f32::EPSILON);
            }
            _ => panic!("expected PeakMeter"),
        }

        match &events[1] {
            EngineEvent::MasterPeakMeter { left, right } => {
                assert!((left - 0.9).abs() < f32::EPSILON);
                assert!((right - 0.7).abs() < f32::EPSILON);
            }
            _ => panic!("expected MasterPeakMeter"),
        }

        match &events[2] {
            EngineEvent::CpuLoad(load) => {
                assert!((load - 0.45).abs() < f32::EPSILON);
            }
            _ => panic!("expected CpuLoad"),
        }
    }

    #[test]
    fn engine_event_transport_variants() {
        let events: Vec<EngineEvent> = vec![
            EngineEvent::PlayheadPosition(48000),
            EngineEvent::TransportStateChanged(TransportState::Playing),
            EngineEvent::TransportStateChanged(TransportState::Stopped),
            EngineEvent::TransportStateChanged(TransportState::Paused),
            EngineEvent::TransportStateChanged(TransportState::Recording),
        ];
        assert_eq!(events.len(), 5);

        match &events[0] {
            EngineEvent::PlayheadPosition(pos) => assert_eq!(*pos, 48000),
            _ => panic!("expected PlayheadPosition"),
        }

        match &events[1] {
            EngineEvent::TransportStateChanged(state) => {
                assert_eq!(*state, TransportState::Playing);
            }
            _ => panic!("expected TransportStateChanged"),
        }
    }

    #[test]
    fn engine_event_recording_variants() {
        let track_id = TrackId::new();
        let events: Vec<EngineEvent> = vec![
            EngineEvent::RecordingOverflow { track_id },
            EngineEvent::RecordingComplete { track_id },
        ];
        assert_eq!(events.len(), 2);

        match &events[0] {
            EngineEvent::RecordingOverflow { track_id: id } => assert_eq!(*id, track_id),
            _ => panic!("expected RecordingOverflow"),
        }
    }

    #[test]
    fn engine_event_error_variants() {
        let events: Vec<EngineEvent> = vec![
            EngineEvent::AudioUnderrun,
            EngineEvent::DeviceError(DeviceErrorKind::DeviceDisconnected),
            EngineEvent::DeviceError(DeviceErrorKind::StreamError(StreamErrorCode::Overflow)),
            EngineEvent::DeviceError(DeviceErrorKind::StreamError(StreamErrorCode::Underflow)),
            EngineEvent::DeviceError(DeviceErrorKind::StreamError(StreamErrorCode::DeviceLost)),
            EngineEvent::DeviceError(DeviceErrorKind::StreamError(StreamErrorCode::Unknown)),
            EngineEvent::DeviceError(DeviceErrorKind::UnsupportedSampleRate(192000)),
            EngineEvent::DeviceError(DeviceErrorKind::UnsupportedBufferSize(8192)),
        ];
        assert_eq!(events.len(), 8);
    }

    #[test]
    fn engine_event_clone() {
        let track_id = TrackId::new();
        let event = EngineEvent::PeakMeter {
            track_id,
            left: 0.5,
            right: 0.3,
        };
        let cloned = event.clone();
        match (&event, &cloned) {
            (
                EngineEvent::PeakMeter {
                    left: l1,
                    right: r1,
                    ..
                },
                EngineEvent::PeakMeter {
                    left: l2,
                    right: r2,
                    ..
                },
            ) => {
                assert!((l1 - l2).abs() < f32::EPSILON);
                assert!((r1 - r2).abs() < f32::EPSILON);
            }
            _ => panic!("clone should produce same variant"),
        }
    }

    #[test]
    fn engine_event_debug_format() {
        let event = EngineEvent::AudioUnderrun;
        let debug = format!("{:?}", event);
        assert!(debug.contains("AudioUnderrun"));
    }

    #[test]
    fn stream_error_code_all_variants() {
        let codes = [
            StreamErrorCode::Overflow,
            StreamErrorCode::Underflow,
            StreamErrorCode::DeviceLost,
            StreamErrorCode::Unknown,
        ];
        // Verify they are all distinct.
        for i in 0..codes.len() {
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j]);
            }
        }
    }

    #[test]
    fn stream_error_code_copy() {
        let code = StreamErrorCode::Overflow;
        let copied = code;
        assert_eq!(code, copied);
    }

    #[test]
    fn device_error_kind_all_variants() {
        let errors: Vec<DeviceErrorKind> = vec![
            DeviceErrorKind::DeviceDisconnected,
            DeviceErrorKind::StreamError(StreamErrorCode::Unknown),
            DeviceErrorKind::UnsupportedSampleRate(48000),
            DeviceErrorKind::UnsupportedBufferSize(256),
        ];
        assert_eq!(errors.len(), 4);

        match &errors[2] {
            DeviceErrorKind::UnsupportedSampleRate(rate) => assert_eq!(*rate, 48000),
            _ => panic!("expected UnsupportedSampleRate"),
        }
        match &errors[3] {
            DeviceErrorKind::UnsupportedBufferSize(size) => assert_eq!(*size, 256),
            _ => panic!("expected UnsupportedBufferSize"),
        }
    }
}
