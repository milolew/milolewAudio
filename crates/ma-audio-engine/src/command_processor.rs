//! Command processor — drains the command ring buffer at the start of each audio callback.
//!
//! This runs on the audio thread but only processes the ring buffer drain
//! (no allocations). Parameter changes are applied immediately via atomics.
//! Topology changes are deferred to the graph-build thread.

use std::sync::atomic::Ordering;

use ma_core::commands::EngineCommand;
use ma_core::events::EngineEvent;
use ma_core::ids::TrackId;
use ma_core::parameters::TransportState;

use crate::graph::nodes::track_node::TrackNode;
use crate::graph::AudioGraph;
use crate::transport::Transport;

/// Maximum commands to process per callback.
/// Prevents spending too much time on commands if the UI floods the buffer.
const MAX_COMMANDS_PER_CALLBACK: usize = 64;

/// Process all pending commands from the UI.
///
/// This is called at the beginning of each audio callback, before graph processing.
///
/// # Arguments
/// * `command_consumer` - SPSC ring buffer consumer for incoming commands
/// * `event_producer` - SPSC ring buffer producer for outgoing events
/// * `transport` - The transport state machine
/// * `graph` - The audio graph (for parameter routing)
/// * `tracks` - Track lookup for parameter changes
///
/// # Real-Time Safety
/// This function runs on the audio thread. It only reads from the ring buffer
/// and writes to atomics — no allocations or I/O.
pub fn process_commands(
    command_consumer: &mut rtrb::Consumer<EngineCommand>,
    event_producer: &mut rtrb::Producer<EngineEvent>,
    transport: &mut Transport,
    graph: &mut AudioGraph,
    tracks: &[crate::track::Track],
) -> bool {
    let mut shutdown = false;
    let mut processed = 0;

    while processed < MAX_COMMANDS_PER_CALLBACK {
        let command = match command_consumer.pop() {
            Ok(cmd) => cmd,
            Err(_) => break, // Ring buffer empty
        };

        match command {
            // ── Transport ──
            EngineCommand::Play => {
                transport.play();
                let _ = event_producer
                    .push(EngineEvent::TransportStateChanged(TransportState::Playing));
            }
            EngineCommand::Stop => {
                transport.stop();
                let _ = event_producer
                    .push(EngineEvent::TransportStateChanged(TransportState::Stopped));
            }
            EngineCommand::Pause => {
                transport.pause();
                let _ =
                    event_producer.push(EngineEvent::TransportStateChanged(TransportState::Paused));
            }
            EngineCommand::SetPosition(pos) => {
                transport.set_position(pos);
            }
            EngineCommand::SetTempo(bpm) => {
                transport.set_tempo(bpm);
            }
            EngineCommand::SetLoop {
                start,
                end,
                enabled,
            } => {
                transport.set_loop(start, end, enabled);
            }

            // ── Track parameters ──
            EngineCommand::SetTrackVolume { track_id, volume } => {
                if let Some(track) = find_track(tracks, track_id) {
                    // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
                    track.volume.store(volume, Ordering::Relaxed);
                }
            }
            EngineCommand::SetTrackPan { track_id, pan } => {
                if let Some(track) = find_track(tracks, track_id) {
                    // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
                    track.pan.store(pan, Ordering::Relaxed);
                }
            }
            EngineCommand::SetTrackMute { track_id, mute } => {
                if let Some(track) = find_track(tracks, track_id) {
                    // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
                    track.mute.store(mute, Ordering::Relaxed);
                }
            }
            EngineCommand::SetTrackSolo { track_id, solo } => {
                if let Some(track) = find_track(tracks, track_id) {
                    // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
                    track.solo.store(solo, Ordering::Relaxed);
                }
            }

            // ── Recording ──
            EngineCommand::ArmTrack { track_id, armed } => {
                if let Some(track) = find_track(tracks, track_id) {
                    // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
                    track.record_armed.store(armed, Ordering::Relaxed);
                }
            }
            EngineCommand::StartRecording => {
                transport.start_recording();
                // Set is_recording on all armed track nodes (using cached graph index)
                for track in tracks {
                    if track.record_armed.load(Ordering::Relaxed) {
                        if let Some(idx) = track.track_node_graph_index {
                            set_track_node_recording(graph, idx, true);
                        }
                    }
                }
                let _ = event_producer.push(EngineEvent::TransportStateChanged(
                    TransportState::Recording,
                ));
            }
            EngineCommand::StopRecording => {
                transport.stop_recording();
                // Clear is_recording on all track nodes (using cached graph index)
                for track in tracks {
                    if let Some(idx) = track.track_node_graph_index {
                        set_track_node_recording(graph, idx, false);
                    }
                }
                let _ = event_producer.push(EngineEvent::TransportStateChanged(transport.state()));
            }

            // ── Lifecycle ──
            EngineCommand::Shutdown => {
                shutdown = true;
            }
        }

        processed += 1;
    }

    shutdown
}

/// Find a track by its ID in the tracks slice.
#[inline]
fn find_track(tracks: &[crate::track::Track], id: TrackId) -> Option<&crate::track::Track> {
    tracks.iter().find(|t| t.id == id)
}

/// Set the is_recording flag on a track node using its cached graph index.
#[inline]
fn set_track_node_recording(graph: &mut AudioGraph, graph_index: usize, recording: bool) {
    if let Some(track_node) = graph.node_downcast_mut::<TrackNode>(graph_index) {
        // ORDERING: Release — cross-thread state read by UI with Acquire
        track_node.is_recording.store(recording, Ordering::Release);
    }
}
