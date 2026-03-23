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

use std::time::Instant;

use ma_core::audio_buffer::MAX_CHANNELS;
use ma_core::commands::EngineCommand;
use ma_core::events::EngineEvent;

use crate::command_processor;
use crate::graph::node::ProcessContext;
use crate::graph::topology::AudioGraph;
use crate::track::Track;
use crate::transport::Transport;

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

    /// Index of the InputNode in the graph (for filling capture buffer).
    pub input_node_index: Option<usize>,

    /// Index of the OutputNode in the graph (for reading final output).
    pub output_node_index: Option<usize>,

    /// Sample rate.
    pub sample_rate: f32,

    /// For CPU load measurement.
    pub last_callback_duration: std::time::Duration,

    /// Callback counter for conditional CPU measurement (every 16th callback).
    pub callback_count: u64,
}

/// The audio output callback. Called by cpal for each output buffer.
///
/// # Arguments
/// * `state` - Mutable reference to the callback state
/// * `output` - cpal's interleaved output buffer to fill
/// * `num_frames` - Number of frames in this callback
///
/// # Real-Time Safety
/// This function MUST NOT: allocate, lock, do I/O, panic.
#[inline]
pub fn audio_callback(
    state: &mut CallbackState,
    output: &mut [f32],
    num_frames: u32,
) {
    state.callback_count += 1;
    let measure_cpu = state.callback_count.is_multiple_of(16);
    let start = if measure_cpu { Some(Instant::now()) } else { None };

    // 1. Drain commands
    let shutdown = command_processor::process_commands(
        &mut state.command_consumer,
        &mut state.event_producer,
        &mut state.transport,
        &mut state.graph,
        &state.tracks,
    );

    if shutdown {
        // Fill with silence and return
        output.fill(0.0);
        return;
    }

    // 2. Advance transport
    let playhead = state.transport.advance(num_frames);

    // 3. Compute solo state across all tracks
    let any_solo = state
        .tracks
        .iter()
        .any(|t| t.solo.load(std::sync::atomic::Ordering::Relaxed));

    // 4. Build process context
    let context = ProcessContext {
        sample_rate: state.sample_rate,
        transport_state: state.transport.state(),
        playhead_samples: playhead,
        tempo: state.transport.tempo(),
        buffer_size: num_frames,
        any_solo,
    };

    // 5. Process audio graph
    state.graph.process(&context);

    // 6. Check for recording overflow on track nodes
    for track in &state.tracks {
        if let Some(idx) = state.graph.find_node_index(track.track_node_id) {
            if let Some(track_node) = state
                .graph
                .node_downcast_mut::<crate::graph::nodes::track_node::TrackNode>(idx)
            {
                if track_node
                    .record_overflow
                    .swap(false, std::sync::atomic::Ordering::Relaxed)
                {
                    let _ = state
                        .event_producer
                        .push(EngineEvent::RecordingOverflow { track_id: track.id });
                }
            }
        }
    }

    // 7. Read output from OutputNode
    if let Some(output_idx) = state.output_node_index {
        if let Some(output_node) =
            state.graph.node_downcast_mut::<crate::graph::nodes::output_node::OutputNode>(output_idx)
        {
            output_node.read_output_interleaved(output);
        } else {
            output.fill(0.0);
        }
    } else {
        output.fill(0.0);
    }

    // 8. Send metering events
    send_meter_events(state, &context);

    // 9. Measure CPU load (only every 16th callback to reduce Instant::now() calls)
    if let Some(start) = start {
        let elapsed = start.elapsed();
        state.last_callback_duration = elapsed;
        let budget = std::time::Duration::from_secs_f64(num_frames as f64 / state.sample_rate as f64);
        let cpu_load = elapsed.as_secs_f32() / budget.as_secs_f32();
        let _ = state.event_producer.push(EngineEvent::CpuLoad(cpu_load));
    }
}

/// The audio input callback. Called by cpal for each input buffer.
///
/// Copies the captured input data to the InputNode's buffer so it's
/// available for the next output callback's graph processing.
#[inline]
pub fn input_callback(
    state: &mut CallbackState,
    input: &[f32],
    num_frames: u32,
    channels: usize,
) {
    if let Some(input_idx) = state.input_node_index {
        if let Some(input_node) =
            state.graph.node_downcast_mut::<crate::graph::nodes::input_node::InputNode>(input_idx)
        {
            input_node.fill_from_input(input, channels, num_frames);
        }
    }
}

/// Send peak meter events for all tracks and master.
#[inline]
fn send_meter_events(state: &mut CallbackState, _context: &ProcessContext) {
    // Read peak levels from each track's output buffer
    // In a full implementation, we'd read from the graph's intermediate buffers.
    // For now, send a master peak event based on the output node.

    if let Some(output_idx) = state.output_node_index {
        if let Some(output_node) =
            state.graph.node_downcast_mut::<crate::graph::nodes::output_node::OutputNode>(output_idx)
        {
            let peaks = output_node.output_buffer().peak_levels();
            let _ = state.event_producer.push(EngineEvent::MasterPeakMeter {
                left: peaks[0],
                right: if MAX_CHANNELS > 1 { peaks[1] } else { peaks[0] },
            });
        }
    }
}
