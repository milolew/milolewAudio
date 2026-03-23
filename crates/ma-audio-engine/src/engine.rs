//! AudioEngine — the top-level orchestrator that ties everything together.
//!
//! The engine is created by the application, sets up cpal devices and streams,
//! creates the audio graph, and provides handles for the UI to communicate with
//! the real-time audio thread.

use std::sync::atomic::{AtomicBool, AtomicI64};
use std::sync::Arc;

use ma_core::commands::{EngineCommand, TopologyCommand};
use ma_core::events::EngineEvent;
use ma_core::ids::{NodeId, TrackId};
use ma_core::parameters::TrackConfig;

use crate::callback::CallbackState;
use crate::disk_io::{self, DiskCommand, DiskEvent};
use crate::graph::edge::Edge;
use crate::graph::nodes::input_node::InputNode;
use crate::graph::nodes::mixer_node::MixerNode;
use crate::graph::nodes::output_node::OutputNode;
use crate::graph::topology::AudioGraph;
use crate::track::{self, Track};
use crate::transport::Transport;

/// Ring buffer capacity for commands (UI → Engine).
const COMMAND_RING_SIZE: usize = 256;

/// Ring buffer capacity for events (Engine → UI).
const EVENT_RING_SIZE: usize = 1024;

/// Default buffer size in frames.
const DEFAULT_BUFFER_SIZE: u32 = 256;

/// Configuration for the audio engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Desired sample rate (e.g., 44100 or 48000).
    pub sample_rate: u32,

    /// Desired buffer size in frames (e.g., 64, 128, 256, 512).
    pub buffer_size: u32,

    /// Initial tracks to create.
    pub initial_tracks: Vec<(TrackId, TrackConfig)>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: DEFAULT_BUFFER_SIZE,
            initial_tracks: Vec::new(),
        }
    }
}

/// Handle for the UI to communicate with the audio engine.
///
/// The UI holds this handle and uses it to send commands and read events/state.
/// All communication is lock-free.
pub struct EngineHandle {
    /// Send commands to the audio engine.
    pub command_producer: rtrb::Producer<EngineCommand>,

    /// Receive events from the audio engine.
    pub event_consumer: rtrb::Consumer<EngineEvent>,

    /// Current playhead position (atomic, updated by audio thread).
    pub playhead_position: Arc<AtomicI64>,

    /// Whether the transport is recording (atomic, updated by audio thread).
    pub is_recording: Arc<AtomicBool>,

    /// Disk I/O command sender (for managing recording files).
    pub disk_command_sender: crossbeam_channel::Sender<DiskCommand>,

    /// Disk I/O event receiver (for recording completion notifications).
    pub disk_event_receiver: crossbeam_channel::Receiver<DiskEvent>,

    /// Send topology commands (AddTrack, RemoveTrack, LoadClip, RemoveClip)
    /// to the graph-build thread. These commands involve heap-allocating types
    /// and must NOT be processed on the audio thread.
    pub topology_command_sender: crossbeam_channel::Sender<TopologyCommand>,

    /// Receive topology commands on the graph-build thread (future use).
    pub topology_command_receiver: crossbeam_channel::Receiver<TopologyCommand>,

    /// Track handles for reading atomic parameters from the UI.
    pub tracks: Vec<TrackHandle>,

    /// Engine configuration snapshot.
    pub config: EngineConfig,
}

/// UI-side handle for reading track state.
pub struct TrackHandle {
    pub id: TrackId,
    pub name: String,
    pub volume: Arc<crate::graph::nodes::track_node::AtomicF32>,
    pub pan: Arc<crate::graph::nodes::track_node::AtomicF32>,
    pub mute: Arc<AtomicBool>,
    pub solo: Arc<AtomicBool>,
    pub record_armed: Arc<AtomicBool>,
}

/// Node ID counter for assigning unique IDs to graph nodes.
struct NodeIdCounter(u32);

impl NodeIdCounter {
    fn new() -> Self {
        Self(0)
    }

    fn next(&mut self) -> NodeId {
        let id = NodeId(self.0);
        self.0 += 1;
        id
    }
}

/// Build the audio engine and return the callback state + UI handle.
///
/// This is called during application startup, OFF the audio thread.
/// All memory is allocated here.
///
/// # Returns
/// `(CallbackState, EngineHandle)` — the callback state is moved into
/// the cpal closure, the handle is given to the UI.
pub fn build_engine(config: EngineConfig) -> (CallbackState, EngineHandle) {
    let mut node_counter = NodeIdCounter::new();

    // Create ring buffers for UI ↔ Engine communication
    let (command_producer, command_consumer) =
        rtrb::RingBuffer::new(COMMAND_RING_SIZE);
    let (event_producer, event_consumer) =
        rtrb::RingBuffer::new(EVENT_RING_SIZE);

    // Create disk I/O thread
    let (disk_cmd_tx, disk_evt_rx) = disk_io::spawn_disk_io_thread();

    // Create topology command channel (for graph-build thread, future use)
    let (topology_cmd_tx, topology_cmd_rx) = crossbeam_channel::unbounded::<TopologyCommand>();

    // Create transport
    let transport = Transport::new(config.sample_rate as f64);
    let playhead_position = transport.position_atomic();
    let is_recording = transport.is_recording_atomic();

    // Create graph nodes
    let input_node_id = node_counter.next();
    let input_node = InputNode::new(input_node_id);

    let mixer_node_id = node_counter.next();
    let mixer_node = MixerNode::new(mixer_node_id, config.initial_tracks.len());

    let output_node_id = node_counter.next();
    let output_node = OutputNode::new(output_node_id);

    // Create tracks and their nodes
    let mut all_nodes: Vec<Box<dyn crate::graph::node::AudioNode>> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut tracks: Vec<Track> = Vec::new();
    let mut track_handles: Vec<TrackHandle> = Vec::new();

    // Add input node first (source)
    let _input_node_index = 0;
    all_nodes.push(Box::new(input_node));

    for (track_id, track_config) in &config.initial_tracks {
        let player_node_id = node_counter.next();
        let track_node_id = node_counter.next();

        let result = track::create_track(
            *track_id,
            track_config.clone(),
            player_node_id,
            track_node_id,
        );

        // Create track handle for UI
        track_handles.push(TrackHandle {
            id: *track_id,
            name: track_config.name.clone(),
            volume: Arc::clone(&result.track.volume),
            pan: Arc::clone(&result.track.pan),
            mute: Arc::clone(&result.track.mute),
            solo: Arc::clone(&result.track.solo),
            record_armed: Arc::clone(&result.track.record_armed),
        });

        // Connect: InputNode → TrackNode (if recording-capable)
        if track_config.input_enabled {
            edges.push(Edge {
                from_node: input_node_id,
                from_port: 0,
                to_node: track_node_id,
                to_port: 0,
            });
        } else {
            // Connect: WavPlayerNode → TrackNode
            edges.push(Edge {
                from_node: player_node_id,
                from_port: 0,
                to_node: track_node_id,
                to_port: 0,
            });
        }

        // Connect: TrackNode → MixerNode
        edges.push(Edge {
            from_node: track_node_id,
            from_port: 0,
            to_node: mixer_node_id,
            to_port: tracks.len(),
        });

        // Add nodes to the graph
        all_nodes.push(Box::new(result.player_node));
        all_nodes.push(Box::new(result.track_node));

        tracks.push(result.track);

        // If there's a record consumer, it will be sent to disk thread when recording starts
        // For now, we don't start recording at engine init
    }

    // Add mixer and output nodes
    all_nodes.push(Box::new(mixer_node));
    let _mixer_index = all_nodes.len() - 1;
    all_nodes.push(Box::new(output_node));
    let _output_index = all_nodes.len() - 1;

    // Connect: MixerNode → OutputNode
    edges.push(Edge {
        from_node: mixer_node_id,
        from_port: 0,
        to_node: output_node_id,
        to_port: 0,
    });

    // Build the audio graph
    let graph = AudioGraph::new(all_nodes, edges, config.buffer_size);

    // Find node indices for the callback
    let input_node_graph_index = graph.find_node_index(input_node_id);
    let output_node_graph_index = graph.find_node_index(output_node_id);

    let callback_state = CallbackState {
        command_consumer,
        event_producer,
        graph,
        transport,
        tracks,
        input_node_index: input_node_graph_index,
        output_node_index: output_node_graph_index,
        sample_rate: config.sample_rate as f32,
        last_callback_duration: std::time::Duration::ZERO,
        callback_count: 0,
    };

    let handle = EngineHandle {
        command_producer,
        event_consumer,
        playhead_position,
        is_recording,
        disk_command_sender: disk_cmd_tx,
        disk_event_receiver: disk_evt_rx,
        topology_command_sender: topology_cmd_tx,
        topology_command_receiver: topology_cmd_rx,
        tracks: track_handles,
        config,
    };

    (callback_state, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_core::parameters::TransportState;

    #[test]
    fn build_engine_with_no_tracks() {
        let config = EngineConfig::default();
        let (state, handle) = build_engine(config);

        assert!(state.tracks.is_empty());
        assert!(handle.tracks.is_empty());
        assert!(state.output_node_index.is_some());
    }

    #[test]
    fn build_engine_with_tracks() {
        let track1_id = TrackId::new();
        let track2_id = TrackId::new();

        let config = EngineConfig {
            sample_rate: 48000,
            buffer_size: 256,
            initial_tracks: vec![
                (
                    track1_id,
                    TrackConfig {
                        name: "Audio 1".into(),
                        channel_count: 2,
                        input_enabled: true,
                        initial_volume: 0.8,
                        initial_pan: 0.0,
                    },
                ),
                (
                    track2_id,
                    TrackConfig {
                        name: "Audio 2".into(),
                        channel_count: 2,
                        input_enabled: false,
                        initial_volume: 1.0,
                        initial_pan: 0.0,
                    },
                ),
            ],
        };

        let (state, handle) = build_engine(config);

        assert_eq!(state.tracks.len(), 2);
        assert_eq!(handle.tracks.len(), 2);
        assert_eq!(handle.tracks[0].name, "Audio 1");
        assert_eq!(handle.tracks[1].name, "Audio 2");
    }

    #[test]
    fn engine_handle_sends_commands() {
        let config = EngineConfig::default();
        let (mut state, mut handle) = build_engine(config);

        // Send a command from UI
        handle
            .command_producer
            .push(EngineCommand::Play)
            .unwrap();

        // Process on audio thread
        crate::command_processor::process_commands(
            &mut state.command_consumer,
            &mut state.event_producer,
            &mut state.transport,
            &mut state.graph,
            &state.tracks,
        );

        assert_eq!(state.transport.state(), TransportState::Playing);

        // Check event was sent back
        let event = handle.event_consumer.pop().unwrap();
        assert!(matches!(event, EngineEvent::TransportStateChanged(TransportState::Playing)));
    }
}
