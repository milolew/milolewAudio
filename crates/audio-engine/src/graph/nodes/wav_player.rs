//! WAV file player node — plays pre-loaded audio clips on the timeline.
//!
//! This node has 0 inputs and 1 stereo output.
//! Audio data is pre-loaded into memory as `Arc<[f32]>` (non-interleaved)
//! by the file loader thread. The audio thread only reads this data.

use std::any::Any;
use std::sync::Arc;

use common_types::audio_buffer::AudioBuffer;
use common_types::ids::{ClipId, NodeId};
use common_types::parameters::TransportState;
use common_types::time::SamplePos;

use crate::graph::node::{AudioNode, ProcessContext};

/// A single audio clip loaded into memory, ready for playback.
pub struct AudioClipRef {
    pub clip_id: ClipId,
    /// Non-interleaved audio data: channel 0 = `[0..length]`, channel 1 = `[length..2*length]`.
    /// Shared with the project state via Arc — audio thread never drops the last reference.
    pub data: Arc<[f32]>,
    /// Number of channels in the clip.
    pub channels: usize,
    /// Start position on the timeline (in samples).
    pub start_sample: SamplePos,
    /// Length of the clip in samples.
    pub length_samples: SamplePos,
}

/// Plays pre-loaded audio clips based on the transport's playhead position.
pub struct WavPlayerNode {
    id: NodeId,
    /// All clips assigned to this player. Pre-allocated, never resized on audio thread.
    /// Maximum capacity set at construction.
    clips: Vec<AudioClipRef>,
    /// Maximum number of clips (pre-allocated capacity).
    max_clips: usize,
}

impl WavPlayerNode {
    pub fn new(id: NodeId, max_clips: usize) -> Self {
        let mut clips = Vec::with_capacity(max_clips);
        clips.clear();
        Self {
            id,
            clips,
            max_clips,
        }
    }

    /// Add a clip to this player. Called from command processor (beginning of audio callback).
    ///
    /// Returns false if the clip list is at capacity (should not happen in normal operation).
    pub fn add_clip(&mut self, clip: AudioClipRef) -> bool {
        if self.clips.len() >= self.max_clips {
            return false;
        }
        self.clips.push(clip);
        true
    }

    /// Remove a clip by ID. Called from command processor.
    pub fn remove_clip(&mut self, clip_id: ClipId) {
        self.clips.retain(|c| c.clip_id != clip_id);
    }

    /// Render audio for the current playhead position into the output buffer.
    #[inline]
    fn render_clips(&self, output: &mut AudioBuffer, context: &ProcessContext) {
        output.clear();

        if context.transport_state == TransportState::Stopped {
            return;
        }

        let playhead = context.playhead_samples;
        let frames = context.buffer_size as SamplePos;

        for clip in &self.clips {
            let clip_end = clip.start_sample + clip.length_samples;

            // Check if this clip overlaps with the current buffer window
            if playhead + frames <= clip.start_sample || playhead >= clip_end {
                continue;
            }

            // Calculate the overlap region
            let buffer_start = (clip.start_sample - playhead).max(0) as usize;
            let clip_offset = (playhead - clip.start_sample).max(0) as usize;
            let available = (clip.length_samples as usize).saturating_sub(clip_offset);
            let copy_frames = available.min(frames as usize - buffer_start);

            // Copy audio data from clip to output buffer
            let channels = clip.channels.min(output.channels());
            for ch in 0..channels {
                let clip_ch_offset = ch * clip.length_samples as usize;
                let src_start = clip_ch_offset + clip_offset;
                let src_end = src_start + copy_frames;

                if src_end <= clip.data.len() {
                    let src = &clip.data[src_start..src_end];
                    let dst = output.channel_mut(ch);
                    for (i, &sample) in src.iter().enumerate() {
                        // Additive mixing — multiple clips can overlap
                        dst[buffer_start + i] += sample;
                    }
                }
            }
        }
    }
}

impl AudioNode for WavPlayerNode {
    fn process(
        &mut self,
        _inputs: &[&AudioBuffer],
        outputs: &mut [&mut AudioBuffer],
        context: &ProcessContext,
    ) {
        if let Some(output) = outputs.first_mut() {
            self.render_clips(output, context);
        }
    }

    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        // No internal state to reset (clips are data, not state)
    }

    fn node_id(&self) -> NodeId {
        self.id
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
