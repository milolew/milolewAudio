//! Track processing node — applies volume, pan, mute/solo and handles recording.
//!
//! This node has 1 input and 1 output (both stereo).
//! It reads parameters via atomics (shared with UI) and optionally
//! pushes audio to a recording ring buffer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use common_types::audio_buffer::AudioBuffer;
use common_types::ids::{NodeId, TrackId};

use crate::graph::node::{AudioNode, ProcessContext};

/// Atomic f32 using AtomicU32 bit representation.
/// This avoids the overhead of `Arc<Mutex<f32>>` and is real-time safe.
pub struct AtomicF32 {
    inner: std::sync::atomic::AtomicU32,
}

impl AtomicF32 {
    pub fn new(value: f32) -> Self {
        Self {
            inner: std::sync::atomic::AtomicU32::new(value.to_bits()),
        }
    }

    #[inline]
    pub fn load(&self, ordering: Ordering) -> f32 {
        f32::from_bits(self.inner.load(ordering))
    }

    #[inline]
    pub fn store(&self, value: f32, ordering: Ordering) {
        self.inner.store(value.to_bits(), ordering);
    }
}

/// Per-track audio processing: gain, pan, mute, and recording.
pub struct TrackNode {
    id: NodeId,
    track_id: TrackId,

    /// Volume (linear gain). Shared with UI via atomic.
    pub volume: Arc<AtomicF32>,

    /// Pan position (-1.0 left, 0.0 center, 1.0 right). Shared with UI via atomic.
    pub pan: Arc<AtomicF32>,

    /// Mute state. Shared with UI via atomic.
    pub mute: Arc<AtomicBool>,

    /// Solo state. Shared with UI via atomic.
    pub solo: Arc<AtomicBool>,

    /// Whether this track is armed for recording.
    pub record_armed: Arc<AtomicBool>,

    /// Whether the transport is currently recording.
    pub is_recording: Arc<AtomicBool>,

    /// SPSC ring buffer producer for sending recorded audio to disk thread.
    /// `None` if this track has no recording capability.
    record_producer: Option<rtrb::Producer<f32>>,
}

impl TrackNode {
    pub fn new(
        id: NodeId,
        track_id: TrackId,
        record_producer: Option<rtrb::Producer<f32>>,
    ) -> Self {
        Self {
            id,
            track_id,
            volume: Arc::new(AtomicF32::new(1.0)),
            pan: Arc::new(AtomicF32::new(0.0)),
            mute: Arc::new(AtomicBool::new(false)),
            solo: Arc::new(AtomicBool::new(false)),
            record_armed: Arc::new(AtomicBool::new(false)),
            is_recording: Arc::new(AtomicBool::new(false)),
            record_producer,
        }
    }

    /// Get the track ID this node belongs to.
    pub fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Push audio samples to the recording ring buffer.
    /// Returns the number of samples that were dropped due to buffer overflow.
    #[inline]
    fn push_to_record_buffer(&mut self, buffer: &AudioBuffer) -> usize {
        let producer = match &mut self.record_producer {
            Some(p) => p,
            None => return 0,
        };

        let frames = buffer.frames() as usize;
        let channels = buffer.channels();
        let mut dropped = 0;

        // Write non-interleaved: all of channel 0, then all of channel 1
        for ch in 0..channels {
            for &sample in buffer.channel(ch) {
                if producer.push(sample).is_err() {
                    dropped += 1;
                }
            }
        }

        // Also write frame count and channel count as metadata?
        // No — the disk thread knows the format (same channels, continuous stream).
        // It just reads raw f32 samples in the same non-interleaved order.

        let _ = frames; // suppress unused warning in release
        dropped
    }
}

impl AudioNode for TrackNode {
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

        // If muted, output silence
        if self.mute.load(Ordering::Relaxed) {
            output.clear();
            return;
        }

        // Copy input to output
        if let Some(input) = inputs.first() {
            output.copy_from(input);
        } else {
            output.clear();
            return;
        }

        // Push to recording buffer if armed and recording
        if self.record_armed.load(Ordering::Relaxed)
            && self.is_recording.load(Ordering::Relaxed)
        {
            // Record the raw input (pre-fader) for clean recording
            if let Some(input) = inputs.first() {
                let _dropped = self.push_to_record_buffer(input);
                // If dropped > 0, the engine should send a RecordingOverflow event.
                // That's handled by the command processor checking ring buffer state.
            }
        }

        // Apply volume
        let vol = self.volume.load(Ordering::Relaxed);
        output.apply_gain(vol);

        // Apply pan
        let pan = self.pan.load(Ordering::Relaxed);
        if pan.abs() > 0.001 {
            output.apply_pan(pan);
        }
    }

    fn input_count(&self) -> usize {
        1
    }

    fn output_count(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        // Nothing to reset — parameters are persistent
    }

    fn node_id(&self) -> NodeId {
        self.id
    }
}
