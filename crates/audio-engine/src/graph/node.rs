//! The core `AudioNode` trait that all graph nodes implement.

use std::any::Any;

use common_types::audio_buffer::AudioBuffer;
use common_types::ids::NodeId;
use common_types::parameters::TransportState;
use common_types::time::{FrameCount, SamplePos};

/// Read-only context passed to every node during processing.
/// Contains transport state, timing, and sample rate information.
#[derive(Debug, Clone, Copy)]
pub struct ProcessContext {
    /// Audio sample rate (e.g., 48000.0).
    pub sample_rate: f32,

    /// Current transport state.
    pub transport_state: TransportState,

    /// Current playhead position in samples.
    pub playhead_samples: SamplePos,

    /// Current tempo in BPM.
    pub tempo: f64,

    /// Number of frames in this callback buffer.
    pub buffer_size: FrameCount,

    /// Whether any track in the project has solo enabled.
    /// Track nodes use this to mute themselves if they are not soloed.
    pub any_solo: bool,
}

/// Trait for all nodes in the audio processing graph.
///
/// Every implementor MUST be real-time safe:
/// - No heap allocations in `process()`
/// - No mutex/rwlock
/// - No file I/O
/// - No panics — handle all errors internally, output silence
///
/// Nodes are `Send` because they are built on one thread and used on the audio thread.
pub trait AudioNode: Send {
    /// Process one buffer of audio.
    ///
    /// # Arguments
    /// * `inputs` - Read-only input buffers (one per input port)
    /// * `outputs` - Mutable output buffers (one per output port)
    /// * `context` - Transport and timing information
    ///
    /// # Real-Time Safety
    /// This method is called on the audio thread. It MUST complete within the
    /// buffer period (e.g., ~5.3ms at 256 samples / 48kHz).
    fn process(
        &mut self,
        inputs: &[&AudioBuffer],
        outputs: &mut [&mut AudioBuffer],
        context: &ProcessContext,
    );

    /// Number of input ports this node accepts.
    fn input_count(&self) -> usize;

    /// Number of output ports this node produces.
    fn output_count(&self) -> usize;

    /// Reset internal state (e.g., clear delay lines, reset phase).
    fn reset(&mut self);

    /// This node's unique identifier within the graph.
    fn node_id(&self) -> NodeId;

    /// Downcast to `&dyn Any` for safe type-checked downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Downcast to `&mut dyn Any` for safe type-checked downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
