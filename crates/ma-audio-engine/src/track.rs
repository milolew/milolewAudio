//! Track management — high-level track state for the engine.
//!
//! Each Track owns its audio graph nodes and recording ring buffer.
//! The Track struct bridges the gap between the project-level track concept
//! and the per-sample graph processing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ma_core::ids::{NodeId, TrackId};
use ma_core::parameters::TrackConfig;

use crate::graph::nodes::track_node::{AtomicF32, TrackNode};
use crate::graph::nodes::wav_player::WavPlayerNode;

/// Default recording ring buffer capacity: ~10 seconds at 48kHz stereo.
/// 48000 samples/sec × 2 channels × 10 sec = 960,000 f32 values ≈ 3.7 MB.
const RECORD_RING_CAPACITY: usize = 960_000;

/// Maximum clips per track for pre-allocation.
const MAX_CLIPS_PER_TRACK: usize = 256;

/// High-level track state that the engine manages.
///
/// A track owns:
/// - A `WavPlayerNode` for playback
/// - A `TrackNode` for gain/pan/recording
/// - A recording ring buffer (producer side — consumer held by disk thread)
/// - Shared atomic parameters readable by the UI
pub struct Track {
    pub id: TrackId,
    pub config: TrackConfig,

    /// Node IDs for this track's graph nodes.
    pub player_node_id: NodeId,
    pub track_node_id: NodeId,

    /// Shared parameter handles (UI reads these atomics).
    pub volume: Arc<AtomicF32>,
    pub pan: Arc<AtomicF32>,
    pub mute: Arc<AtomicBool>,
    pub solo: Arc<AtomicBool>,
    pub record_armed: Arc<AtomicBool>,
}

/// Result of creating a new track — contains the nodes and recording consumer.
pub struct TrackCreationResult {
    /// The WAV player node for this track.
    pub player_node: WavPlayerNode,

    /// The track processing node (gain, pan, recording).
    pub track_node: TrackNode,

    /// The recording ring buffer consumer (to be given to the disk I/O thread).
    /// `None` if the track has no recording capability.
    pub record_consumer: Option<rtrb::Consumer<f32>>,

    /// The high-level track state.
    pub track: Track,
}

/// Create a new track with all its components.
///
/// This allocates the ring buffer and creates the graph nodes.
/// Must be called OFF the audio thread (allocations happen here).
///
/// # Arguments
/// * `track_id` - Unique ID for this track
/// * `config` - Track configuration
/// * `player_node_id` - NodeId for the WAV player node
/// * `track_node_id` - NodeId for the track processing node
pub fn create_track(
    track_id: TrackId,
    config: TrackConfig,
    player_node_id: NodeId,
    track_node_id: NodeId,
) -> TrackCreationResult {
    // Create recording ring buffer if input is enabled
    let (record_producer, record_consumer) = if config.input_enabled {
        let (producer, consumer) = rtrb::RingBuffer::new(RECORD_RING_CAPACITY);
        (Some(producer), Some(consumer))
    } else {
        (None, None)
    };

    // Create the WAV player node
    let player_node = WavPlayerNode::new(player_node_id, MAX_CLIPS_PER_TRACK);

    // Create the track processing node
    let track_node = TrackNode::new(track_node_id, track_id, record_producer);

    // Set initial parameter values
    track_node
        .volume
        .store(config.initial_volume, Ordering::Relaxed);
    track_node
        .pan
        .store(config.initial_pan, Ordering::Relaxed);

    // Clone Arc handles for UI access
    let volume = Arc::clone(&track_node.volume);
    let pan = Arc::clone(&track_node.pan);
    let mute = Arc::clone(&track_node.mute);
    let solo = Arc::clone(&track_node.solo);
    let record_armed = Arc::clone(&track_node.record_armed);

    let track = Track {
        id: track_id,
        config,
        player_node_id,
        track_node_id,
        volume,
        pan,
        mute,
        solo,
        record_armed,
    };

    TrackCreationResult {
        player_node,
        track_node,
        record_consumer,
        track,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_track_with_recording() {
        let result = create_track(
            TrackId::new(),
            TrackConfig {
                name: "Test Track".into(),
                channel_count: 2,
                input_enabled: true,
                initial_volume: 0.8,
                initial_pan: -0.5,
            },
            NodeId(0),
            NodeId(1),
        );

        assert!(result.record_consumer.is_some());
        assert_eq!(
            result.track.volume.load(Ordering::Relaxed),
            0.8
        );
        assert_eq!(
            result.track.pan.load(Ordering::Relaxed),
            -0.5
        );
    }

    #[test]
    fn create_track_without_recording() {
        let result = create_track(
            TrackId::new(),
            TrackConfig {
                input_enabled: false,
                ..TrackConfig::default()
            },
            NodeId(0),
            NodeId(1),
        );

        assert!(result.record_consumer.is_none());
    }
}
