//! Audio callback — the real-time audio processing entry point.
//!
//! This module provides the function that cpal calls for each audio buffer.
//! It is the HOT PATH — every nanosecond counts here.
//!
//! # Real-Time Guarantee
//! At 48kHz with 256-frame buffers, we have ~5.33ms to:
//! 1. Drain command ring buffer
//! 2. Advance transport
//! 3. Process entire audio graph
//! 4. Send meter events
//! 5. Copy output to cpal buffer

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::time::Instant;

use ma_core::audio_buffer::MAX_CHANNELS;
use ma_core::commands::EngineCommand;
use ma_core::events::EngineEvent;
use ma_core::ids::TrackId;

use crate::command_processor;
use crate::graph::node::ProcessContext;
use crate::graph::topology::AudioGraph;
use crate::input_capture::InputCaptureReader;
use crate::track::Track;
use crate::transport::Transport;

/// Count of events dropped due to event ring buffer overflow.
/// Incremented on the audio thread, read/reset from UI thread.
static DROPPED_EVENTS: AtomicU32 = AtomicU32::new(0);

/// Take the current dropped event count, resetting it to zero.
/// Call this periodically from the UI thread.
pub fn take_dropped_event_count() -> u32 {
    DROPPED_EVENTS.swap(0, Ordering::Relaxed)
}

/// State held by the audio callback closure.
///
/// This struct is moved into the cpal callback closure.
/// All fields are either owned values or lock-free handles.
pub struct CallbackState {
    /// Command ring buffer consumer (UI → Engine).
    pub command_consumer: rtrb::Consumer<EngineCommand>,

    /// Event ring buffer producer (Engine → UI).
    pub event_producer: rtrb::Producer<EngineEvent>,

    /// The audio processing graph.
    pub graph: AudioGraph,

    /// Transport (playhead, state machine).
    pub transport: Transport,

    /// Track metadata (for command routing).
    pub tracks: Vec<Track>,

    /// Pre-built index for O(1) track lookup by TrackId.
    /// Built off-thread in build_engine(), read-only on audio thread.
    pub track_index: HashMap<TrackId, usize>,

    /// Index of the InputNode in the graph (for filling capture buffer).
    pub input_node_index: Option<usize>,

    /// Index of the OutputNode in the graph (for reading final output).
    pub output_node_index: Option<usize>,

    /// Sample rate.
    pub sample_rate: f32,

    /// Reader for input capture ring buffer (filled by cpal input callback).
    /// `None` if no input device is active.
    pub input_capture_reader: Option<InputCaptureReader>,

    /// For CPU load measurement.
    pub last_callback_duration: std::time::Duration,

    /// Callback counter for conditional CPU measurement (every 16th callback).
    pub callback_count: u64,

    /// Set to `true` if the audio callback panicked. Once set, all subsequent
    /// callbacks output silence. The UI should check this flag and show an error.
    pub has_panicked: AtomicBool,

    /// Set by cpal stream error callbacks to signal device errors.
    /// The audio callback checks this flag and forwards it as an EngineEvent.
    /// 0 = no error. Non-zero value is a `StreamErrorCode` discriminant + 1.
    pub device_error_flag: std::sync::Arc<AtomicU8>,
}

// SAFETY: CallbackState is moved into the cpal audio callback closure and
// accessed exclusively from the audio thread after that point.
// AudioGraph contains Vec<Box<dyn AudioNode>> which is Send (trait bound),
// and Vec<AudioBuffer> which is Send. The rtrb Consumer/Producer types
// are Send (designed for cross-thread transfer). All other fields are
// Send-safe primitives or Option wrappers thereof.
// No concurrent access occurs after the move.
unsafe impl Send for CallbackState {}

/// The audio output callback. Called by cpal for each output buffer.
///
/// Wraps the actual processing in `catch_unwind` so that a panic on the
/// audio thread does not abort the process. On panic, the output buffer is
/// filled with silence and `has_panicked` is set permanently.
///
/// # Arguments
/// * `state` - Mutable reference to the callback state
/// * `output` - cpal's interleaved output buffer to fill
/// * `num_frames` - Number of frames in this callback
#[inline]
pub fn audio_callback(state: &mut CallbackState, output: &mut [f32], num_frames: u32) {
    // If a previous callback panicked, output silence permanently.
    // ORDERING: Relaxed OK — single-value flag, only transitions false→true
    if state.has_panicked.load(Ordering::Relaxed) {
        output.fill(0.0);
        return;
    }

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        audio_callback_inner(state, output, num_frames)
    }));

    if result.is_err() {
        output.fill(0.0);
        // ORDERING: Release — UI thread reads this with Acquire
        state.has_panicked.store(true, Ordering::Release);
        // Best-effort notification to UI (ring buffer push won't panic)
        push_event(&mut state.event_producer, EngineEvent::AudioThreadPanic);
    }
}

/// The actual audio callback body. Separated from `audio_callback` so that
/// `catch_unwind` can wrap it without nesting.
///
/// # Real-Time Safety
/// This function MUST NOT: allocate, lock, do I/O.
#[inline]
fn audio_callback_inner(state: &mut CallbackState, output: &mut [f32], num_frames: u32) {
    state.callback_count += 1;
    let measure_cpu = state.callback_count.is_multiple_of(16);
    let start = if measure_cpu {
        Some(Instant::now())
    } else {
        None
    };

    // 1. Drain commands
    let shutdown = command_processor::process_commands(
        &mut state.command_consumer,
        &mut state.event_producer,
        &mut state.transport,
        &mut state.graph,
        &state.tracks,
        &state.track_index,
    );

    if shutdown {
        // Fill with silence and return
        output.fill(0.0);
        return;
    }

    // 1b. Check for device errors reported by cpal error callbacks
    let error_code = state.device_error_flag.swap(0, Ordering::Relaxed);
    if error_code != 0 {
        use ma_core::events::DeviceErrorKind;
        use ma_core::events::StreamErrorCode;
        let code = match error_code {
            1 => StreamErrorCode::Overflow,
            2 => StreamErrorCode::Underflow,
            3 => StreamErrorCode::DeviceLost,
            _ => StreamErrorCode::Unknown,
        };
        push_event(
            &mut state.event_producer,
            EngineEvent::DeviceError(DeviceErrorKind::StreamError(code)),
        );
    }

    // 2. Drain input capture ring buffer into InputNode
    if let (Some(reader), Some(input_idx)) =
        (&mut state.input_capture_reader, state.input_node_index)
    {
        let channels = reader.channel_count();
        let interleaved = reader.drain_into_staging(num_frames);
        if let Some(input_node) = state
            .graph
            .node_downcast_mut::<crate::graph::nodes::input_node::InputNode>(input_idx)
        {
            input_node.fill_from_input(interleaved, channels, num_frames);
        }
    }

    // 3. Advance transport
    let playhead = state.transport.advance(num_frames);

    // 4. Compute solo state across all tracks
    let any_solo = state
        .tracks
        .iter()
        // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
        .any(|t| t.solo.load(std::sync::atomic::Ordering::Relaxed));

    // 5. Build process context
    let context = ProcessContext {
        sample_rate: state.sample_rate,
        transport_state: state.transport.state(),
        playhead_samples: playhead,
        tempo: state.transport.tempo(),
        buffer_size: num_frames,
        any_solo,
    };

    // 6. Process audio graph
    state.graph.process(&context);

    // 7. Check for recording overflow on track nodes (using cached graph indices)
    for track in &state.tracks {
        if let Some(idx) = track.track_node_graph_index {
            if let Some(track_node) = state
                .graph
                .node_downcast_mut::<crate::graph::nodes::track_node::TrackNode>(idx)
            {
                if track_node
                    .record_overflow
                    // ORDERING: Relaxed OK — single-value flag, set/reset within audio thread
                    .swap(false, std::sync::atomic::Ordering::Relaxed)
                {
                    push_event(
                        &mut state.event_producer,
                        EngineEvent::RecordingOverflow { track_id: track.id },
                    );
                }
            }
        }
    }

    // 8. Read output from OutputNode
    if let Some(output_idx) = state.output_node_index {
        if let Some(output_node) = state
            .graph
            .node_downcast_mut::<crate::graph::nodes::output_node::OutputNode>(output_idx)
        {
            output_node.read_output_interleaved(output);
        } else {
            output.fill(0.0);
        }
    } else {
        output.fill(0.0);
    }

    // 9. Send metering events
    send_meter_events(state, &context);

    // 10. Measure CPU load (only every 16th callback to reduce Instant::now() calls)
    if let Some(start) = start {
        let elapsed = start.elapsed();
        state.last_callback_duration = elapsed;
        let budget =
            std::time::Duration::from_secs_f64(num_frames as f64 / state.sample_rate as f64);
        let cpu_load = elapsed.as_secs_f32() / budget.as_secs_f32();
        push_event(&mut state.event_producer, EngineEvent::CpuLoad(cpu_load));
    }
}

/// Push an event to the UI, counting drops on overflow.
#[inline]
pub(crate) fn push_event(producer: &mut rtrb::Producer<EngineEvent>, event: EngineEvent) {
    if producer.push(event).is_err() {
        DROPPED_EVENTS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Send peak meter events for all tracks and master.
#[inline]
fn send_meter_events(state: &mut CallbackState, _context: &ProcessContext) {
    // Read peak levels from each track's output buffer
    // In a full implementation, we'd read from the graph's intermediate buffers.
    // For now, send a master peak event based on the output node.

    if let Some(output_idx) = state.output_node_index {
        if let Some(output_node) = state
            .graph
            .node_downcast_mut::<crate::graph::nodes::output_node::OutputNode>(output_idx)
        {
            let peaks = output_node.output_buffer().peak_levels();
            push_event(
                &mut state.event_producer,
                EngineEvent::MasterPeakMeter {
                    left: peaks[0],
                    right: if MAX_CHANNELS > 1 { peaks[1] } else { peaks[0] },
                },
            );
        }
    }
}
