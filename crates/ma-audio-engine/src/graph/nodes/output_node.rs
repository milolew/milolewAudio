//! Audio output node — feeds the final mix to cpal output device.
//!
//! This node has 1 stereo input and 0 outputs.
//! After processing, the audio callback reads from this node's internal
//! buffer to fill the cpal output.

use std::any::Any;

use ma_core::audio_buffer::AudioBuffer;
use ma_core::ids::NodeId;

use crate::graph::node::{AudioNode, ProcessContext};

/// Feeds the final mixed audio to the system output device.
///
/// The audio callback reads from `output_buffer` after graph processing
/// to fill cpal's output buffer.
pub struct OutputNode {
    id: NodeId,
    /// Buffer that holds the final mix for cpal to read.
    output_buffer: AudioBuffer,
}

impl OutputNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            output_buffer: AudioBuffer::stereo(0),
        }
    }

    /// Read the final output buffer as interleaved samples for cpal.
    ///
    /// # Arguments
    /// * `output` - Destination buffer in cpal's interleaved format
    #[inline]
    pub fn read_output_interleaved(&self, output: &mut [f32]) {
        self.output_buffer.to_interleaved(output);
    }

    /// Direct read access to the output buffer.
    pub fn output_buffer(&self) -> &AudioBuffer {
        &self.output_buffer
    }
}

impl AudioNode for OutputNode {
    fn process(
        &mut self,
        inputs: &[&AudioBuffer],
        _outputs: &mut [&mut AudioBuffer],
        _context: &ProcessContext,
    ) {
        if let Some(input) = inputs.first() {
            self.output_buffer.copy_from(input);
            // Clamp to prevent DAC clipping
            self.output_buffer.clamp();
        } else {
            self.output_buffer.clear();
        }
    }

    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        0
    }

    fn reset(&mut self) {
        self.output_buffer.clear();
    }

    fn node_id(&self) -> NodeId {
        self.id
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
