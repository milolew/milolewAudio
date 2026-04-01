//! E8: Integration test — recording overflow detection.
//!
//! Verifies that when the recording ring buffer overflows, the engine:
//! 1. Emits a RecordingOverflow event
//! 2. Continues operating (no panic, no deadlock)

use std::sync::atomic::Ordering;

use ma_audio_engine::callback::audio_callback;
use ma_audio_engine::engine::{build_engine, EngineConfig};
use ma_core::commands::EngineCommand;
use ma_core::events::EngineEvent;
use ma_core::ids::TrackId;
use ma_core::parameters::{TrackConfig, TrackType};

#[test]
fn recording_overflow_emits_event_and_engine_continues() {
    // Build engine with a single input-enabled audio track
    let track_id = TrackId::new();
    let config = EngineConfig {
        sample_rate: 48000,
        buffer_size: 256,
        initial_tracks: vec![(
            track_id,
            TrackConfig {
                name: "Overflow Test".into(),
                channel_count: 2,
                input_enabled: true,
                initial_volume: 1.0,
                initial_pan: 0.0,
                track_type: TrackType::Audio,
            },
        )],
    };
    let (mut state, mut handle) = build_engine(config).unwrap();

    // Arm the track
    handle
        .command_producer
        .push(EngineCommand::ArmTrack {
            track_id,
            armed: true,
        })
        .unwrap();

    // Start recording
    handle
        .command_producer
        .push(EngineCommand::StartRecording)
        .unwrap();

    // Run enough callbacks to overflow the recording ring buffer.
    // RECORD_RING_CAPACITY = 960,000 samples.
    // Each callback with 256 frames and 2 channels writes 512 samples.
    // Need ~1,875 callbacks to fill, run 2,100 to guarantee overflow.
    //
    // Drain the event ring buffer periodically to prevent metering events
    // from filling it up and causing RecordingOverflow to be dropped.
    let mut output = vec![0.0f32; 256 * 2];
    let mut found_overflow = false;
    for i in 0..2100 {
        audio_callback(&mut state, &mut output, 256);
        // Drain events every 100 callbacks
        if i % 100 == 99 {
            while let Ok(event) = handle.event_consumer.pop() {
                if matches!(event, EngineEvent::RecordingOverflow { .. }) {
                    found_overflow = true;
                }
            }
        }
    }

    // Final drain
    while let Ok(event) = handle.event_consumer.pop() {
        if matches!(event, EngineEvent::RecordingOverflow { .. }) {
            found_overflow = true;
        }
    }

    assert!(
        found_overflow,
        "Expected RecordingOverflow event after filling the ring buffer"
    );

    // Verify the engine continues operating (no panic)
    assert!(
        !state.has_panicked.load(Ordering::Relaxed),
        "Engine should not panic after recording overflow"
    );

    // Run a few more callbacks to confirm continued operation
    for _ in 0..10 {
        audio_callback(&mut state, &mut output, 256);
    }
    assert!(
        !state.has_panicked.load(Ordering::Relaxed),
        "Engine should still be running after overflow"
    );
}
