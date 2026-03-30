//! Command processor — drains the command ring buffer at the start of each audio callback.
//!
//! This runs on the audio thread but only processes the ring buffer drain
//! (no allocations). Parameter changes are applied immediately via atomics.
//! Topology changes are deferred to the graph-build thread.

use std::sync::atomic::Ordering;

use ma_core::commands::EngineCommand;
use ma_core::events::EngineEvent;

use crate::callback::push_event;
use ma_core::ids::TrackId;
use ma_core::parameters::TransportState;

use crate::graph::nodes::midi_player::MidiPlayerNode;
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
                push_event(
                    event_producer,
                    EngineEvent::TransportStateChanged(TransportState::Playing),
                );
            }
            EngineCommand::Stop => {
                transport.stop();
                push_event(
                    event_producer,
                    EngineEvent::TransportStateChanged(TransportState::Stopped),
                );
            }
            EngineCommand::Pause => {
                transport.pause();
                push_event(
                    event_producer,
                    EngineEvent::TransportStateChanged(TransportState::Paused),
                );
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
                push_event(
                    event_producer,
                    EngineEvent::TransportStateChanged(TransportState::Recording),
                );
            }
            EngineCommand::StopRecording => {
                transport.stop_recording();
                // Clear is_recording on all track nodes (using cached graph index)
                for track in tracks {
                    if let Some(idx) = track.track_node_graph_index {
                        set_track_node_recording(graph, idx, false);
                    }
                }
                push_event(
                    event_producer,
                    EngineEvent::TransportStateChanged(transport.state()),
                );
            }

            // ── Clip management ──
            EngineCommand::InstallMidiClip {
                track_id,
                clip_id,
                clip,
                start_tick,
            } => {
                if let Some(track) = find_track(tracks, track_id) {
                    if let Some(idx) = track.player_node_graph_index {
                        if let Some(player) = graph.node_downcast_mut::<MidiPlayerNode>(idx) {
                            player.add_clip(ma_core::midi_clip::MidiClipRef {
                                clip_id,
                                clip,
                                start_tick,
                            });
                        }
                    }
                }
            }
            EngineCommand::RemoveMidiClipFromPlayer { track_id, clip_id } => {
                if let Some(track) = find_track(tracks, track_id) {
                    if let Some(idx) = track.player_node_graph_index {
                        if let Some(player) = graph.node_downcast_mut::<MidiPlayerNode>(idx) {
                            player.remove_clip(clip_id);
                        }
                    }
                }
            }

            // ── Input monitoring ──
            EngineCommand::SetInputMonitoring {
                track_id,
                monitoring,
            } => {
                if let Some(track) = find_track(tracks, track_id) {
                    if let Some(idx) = track.track_node_graph_index {
                        if let Some(track_node) = graph.node_downcast_mut::<TrackNode>(idx) {
                            track_node
                                .input_monitoring
                                .store(monitoring, Ordering::Relaxed);
                        }
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{build_engine, EngineConfig};

    /// Build a minimal engine with one track for testing.
    fn test_engine() -> (
        rtrb::Producer<EngineCommand>,
        rtrb::Consumer<EngineEvent>,
        crate::callback::CallbackState,
    ) {
        let track_id = TrackId::new();
        let config = EngineConfig {
            sample_rate: 48000,
            buffer_size: 256,
            initial_tracks: vec![(
                track_id,
                ma_core::parameters::TrackConfig {
                    name: "Test".into(),
                    channel_count: 2,
                    input_enabled: true,
                    initial_volume: 1.0,
                    initial_pan: 0.0,
                    track_type: ma_core::parameters::TrackType::Audio,
                },
            )],
        };
        let (state, handle) = build_engine(config).unwrap();
        (handle.command_producer, handle.event_consumer, state)
    }

    fn dispatch(state: &mut crate::callback::CallbackState) -> bool {
        process_commands(
            &mut state.command_consumer,
            &mut state.event_producer,
            &mut state.transport,
            &mut state.graph,
            &state.tracks,
        )
    }

    #[test]
    fn play_command_changes_transport_and_emits_event() {
        let (mut producer, mut consumer, mut state) = test_engine();
        producer.push(EngineCommand::Play).unwrap();
        let shutdown = dispatch(&mut state);
        assert!(!shutdown);
        assert_eq!(state.transport.state(), TransportState::Playing);
        let event = consumer.pop().unwrap();
        assert!(matches!(
            event,
            EngineEvent::TransportStateChanged(TransportState::Playing)
        ));
    }

    #[test]
    fn stop_command_changes_transport() {
        let (mut producer, mut consumer, mut state) = test_engine();
        producer.push(EngineCommand::Play).unwrap();
        dispatch(&mut state);
        consumer.pop().unwrap(); // drain play event
        producer.push(EngineCommand::Stop).unwrap();
        dispatch(&mut state);
        assert_eq!(state.transport.state(), TransportState::Stopped);
        let event = consumer.pop().unwrap();
        assert!(matches!(
            event,
            EngineEvent::TransportStateChanged(TransportState::Stopped)
        ));
    }

    #[test]
    fn pause_command_changes_transport() {
        let (mut producer, mut consumer, mut state) = test_engine();
        producer.push(EngineCommand::Play).unwrap();
        dispatch(&mut state);
        consumer.pop().unwrap();
        producer.push(EngineCommand::Pause).unwrap();
        dispatch(&mut state);
        assert_eq!(state.transport.state(), TransportState::Paused);
    }

    #[test]
    fn set_position_command() {
        let (mut producer, _, mut state) = test_engine();
        producer.push(EngineCommand::SetPosition(12345)).unwrap();
        dispatch(&mut state);
        assert_eq!(state.transport.position(), 12345);
    }

    #[test]
    fn set_tempo_command() {
        let (mut producer, _, mut state) = test_engine();
        producer.push(EngineCommand::SetTempo(140.0)).unwrap();
        dispatch(&mut state);
        assert!((state.transport.tempo() - 140.0).abs() < f64::EPSILON);
    }

    #[test]
    fn set_track_volume_command() {
        let (mut producer, _, mut state) = test_engine();
        let track_id = state.tracks[0].id;
        producer
            .push(EngineCommand::SetTrackVolume {
                track_id,
                volume: 0.5,
            })
            .unwrap();
        dispatch(&mut state);
        let vol = state.tracks[0].volume.load(Ordering::Relaxed);
        assert!((vol - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn set_track_pan_command() {
        let (mut producer, _, mut state) = test_engine();
        let track_id = state.tracks[0].id;
        producer
            .push(EngineCommand::SetTrackPan {
                track_id,
                pan: -0.3,
            })
            .unwrap();
        dispatch(&mut state);
        let pan = state.tracks[0].pan.load(Ordering::Relaxed);
        assert!((pan - (-0.3)).abs() < f32::EPSILON);
    }

    #[test]
    fn mute_and_solo_commands() {
        let (mut producer, _, mut state) = test_engine();
        let track_id = state.tracks[0].id;
        producer
            .push(EngineCommand::SetTrackMute {
                track_id,
                mute: true,
            })
            .unwrap();
        producer
            .push(EngineCommand::SetTrackSolo {
                track_id,
                solo: true,
            })
            .unwrap();
        dispatch(&mut state);
        assert!(state.tracks[0].mute.load(Ordering::Relaxed));
        assert!(state.tracks[0].solo.load(Ordering::Relaxed));
    }

    #[test]
    fn arm_track_command() {
        let (mut producer, _, mut state) = test_engine();
        let track_id = state.tracks[0].id;
        producer
            .push(EngineCommand::ArmTrack {
                track_id,
                armed: true,
            })
            .unwrap();
        dispatch(&mut state);
        assert!(state.tracks[0].record_armed.load(Ordering::Relaxed));
    }

    #[test]
    fn shutdown_command_returns_true() {
        let (mut producer, _, mut state) = test_engine();
        producer.push(EngineCommand::Shutdown).unwrap();
        let shutdown = dispatch(&mut state);
        assert!(shutdown);
    }

    #[test]
    fn start_stop_recording_commands() {
        let (mut producer, mut consumer, mut state) = test_engine();
        let track_id = state.tracks[0].id;
        // Arm the track
        producer
            .push(EngineCommand::ArmTrack {
                track_id,
                armed: true,
            })
            .unwrap();
        dispatch(&mut state);
        // Start recording
        producer.push(EngineCommand::StartRecording).unwrap();
        dispatch(&mut state);
        assert_eq!(state.transport.state(), TransportState::Recording);
        // Drain events
        while consumer.pop().is_ok() {}
        // Stop recording
        producer.push(EngineCommand::StopRecording).unwrap();
        dispatch(&mut state);
        assert_eq!(state.transport.state(), TransportState::Playing);
    }

    #[test]
    fn unknown_track_id_is_ignored() {
        let (mut producer, _, mut state) = test_engine();
        let bogus_id = TrackId::new();
        producer
            .push(EngineCommand::SetTrackVolume {
                track_id: bogus_id,
                volume: 0.5,
            })
            .unwrap();
        // Should not panic
        dispatch(&mut state);
    }

    /// Build a minimal engine with one MIDI track for testing clip installation.
    fn test_engine_midi() -> (
        rtrb::Producer<EngineCommand>,
        rtrb::Consumer<EngineEvent>,
        crate::callback::CallbackState,
    ) {
        let track_id = TrackId::new();
        let config = EngineConfig {
            sample_rate: 48000,
            buffer_size: 256,
            initial_tracks: vec![(
                track_id,
                ma_core::parameters::TrackConfig {
                    name: "MIDI 1".into(),
                    channel_count: 2,
                    input_enabled: false,
                    initial_volume: 1.0,
                    initial_pan: 0.0,
                    track_type: ma_core::parameters::TrackType::Midi,
                },
            )],
        };
        let (state, handle) = build_engine(config).unwrap();
        (handle.command_producer, handle.event_consumer, state)
    }

    #[test]
    fn install_midi_clip_command() {
        let (mut producer, _, mut state) = test_engine_midi();
        let track_id = state.tracks[0].id;
        let clip_id = ma_core::ids::ClipId::new();
        let clip = std::sync::Arc::new(ma_core::midi_clip::MidiClip::new(vec![], 960));

        producer
            .push(EngineCommand::InstallMidiClip {
                track_id,
                clip_id,
                clip,
                start_tick: 0,
            })
            .unwrap();
        dispatch(&mut state);

        // Verify clip was installed by checking the MidiPlayerNode
        let idx = state.tracks[0].player_node_graph_index.unwrap();
        let player = state
            .graph
            .node_downcast_mut::<MidiPlayerNode>(idx)
            .unwrap();
        assert_eq!(player.clip_count(), 1);
    }

    #[test]
    fn remove_midi_clip_command() {
        let (mut producer, _, mut state) = test_engine_midi();
        let track_id = state.tracks[0].id;
        let clip_id = ma_core::ids::ClipId::new();
        let clip = std::sync::Arc::new(ma_core::midi_clip::MidiClip::new(vec![], 960));

        // Install
        producer
            .push(EngineCommand::InstallMidiClip {
                track_id,
                clip_id,
                clip,
                start_tick: 0,
            })
            .unwrap();
        dispatch(&mut state);

        // Remove
        producer
            .push(EngineCommand::RemoveMidiClipFromPlayer { track_id, clip_id })
            .unwrap();
        dispatch(&mut state);

        let idx = state.tracks[0].player_node_graph_index.unwrap();
        let player = state
            .graph
            .node_downcast_mut::<MidiPlayerNode>(idx)
            .unwrap();
        assert_eq!(player.clip_count(), 0);
    }
}
