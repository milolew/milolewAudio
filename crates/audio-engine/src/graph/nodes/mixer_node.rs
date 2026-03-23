//! Mixer node — sums N input buffers into a single stereo output.
//!
//! This is the master bus. It has N inputs (one per track) and 1 output.

use std::any::Any;

use common_types::audio_buffer::AudioBuffer;
use common_types::ids::NodeId;

use crate::graph::node::{AudioNode, ProcessContext};

/// Sums all input buffers into a single output buffer (master mix).
pub struct MixerNode {
    id: NodeId,
    /// Number of input ports (one per track). Set at construction.
    num_inputs: usize,
}

impl MixerNode {
    pub fn new(id: NodeId, num_inputs: usize) -> Self {
        Self { id, num_inputs }
    }

    /// Update the number of inputs (when tracks are added/removed).
    /// Called from command processor, not from the audio thread hot path.
    pub fn set_num_inputs(&mut self, count: usize) {
        self.num_inputs = count;
    }
}

impl AudioNode for MixerNode {
    fn process(
        &mut self,
        inputs: &[&AudioBuffer],
        outputs: &mut [&mut AudioBuffer],
        _context: &ProcessContext,
    ) {
        let output = match outputs.first_mut() {
            Some(o) => o,
            None => return,
        };

        output.clear();

        for input in inputs {
            output.mix_from(input);
        }
    }

    fn input_count(&self) -> usize {
        self.num_inputs
    }

    fn output_count(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        // Stateless node — nothing to reset
    }

    fn node_id(&self) -> NodeId {
        self.id
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
