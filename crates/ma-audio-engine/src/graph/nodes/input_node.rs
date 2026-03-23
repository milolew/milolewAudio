//! Audio input node — captures audio from cpal input device (e.g., M-Audio Duo).
//!
//! This node has 0 inputs and 1 stereo output.
//! It reads from a pre-filled capture buffer that the cpal input callback
//! populates before graph processing begins.

use std::any::Any;

use ma_core::audio_buffer::AudioBuffer;
use ma_core::ids::NodeId;

use crate::graph::node::{AudioNode, ProcessContext};

/// Captures audio from the system input device.
///
/// The cpal input callback copies captured samples into `capture_buffer`
/// before graph processing. This node then makes that data available
/// as its output.
pub struct InputNode {
    id: NodeId,
    /// Buffer filled by cpal input callback before `process()` is called.
    /// This is NOT shared with cpal — the callback copies data here
    /// at the beginning of each audio cycle.
    capture_buffer: AudioBuffer,
}

impl InputNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            capture_buffer: AudioBuffer::stereo(0),
        }
    }

    /// Called by the audio callback to provide captured input data.
    /// This must be called BEFORE `process()` in each audio cycle.
    ///
    /// # Arguments
    /// * `interleaved` - Raw interleaved samples from cpal input callback
    /// * `channels` - Number of channels (typically 2 for M-Audio Duo)
    /// * `frames` - Number of frames in this callback
    #[inline]
    pub fn fill_from_input(&mut self, interleaved: &[f32], channels: usize, frames: u32) {
        self.capture_buffer
            .from_interleaved(interleaved, channels, frames);
    }

    /// Direct access to the capture buffer (for testing or direct buffer fill).
    pub fn capture_buffer_mut(&mut self) -> &mut AudioBuffer {
        &mut self.capture_buffer
    }
}

impl AudioNode for InputNode {
    fn process(
        &mut self,
        _inputs: &[&AudioBuffer],
        outputs: &mut [&mut AudioBuffer],
        _context: &ProcessContext,
    ) {
        if let Some(output) = outputs.first_mut() {
            // If no input was captured yet (frames mismatch), output silence
            if self.capture_buffer.frames() == output.frames() {
                output.copy_from(&self.capture_buffer);
            } else {
                output.clear();
            }
        }
    }

    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.capture_buffer.clear();
    }

    fn node_id(&self) -> NodeId {
        self.id
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
