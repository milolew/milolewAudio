//! Track processing node — applies volume, pan, mute/solo and handles recording.
//!
//! This node has 1 input and 1 output (both stereo).
//! It reads parameters via atomics (shared with UI) and optionally
//! pushes audio to a recording ring buffer.

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ma_core::audio_buffer::AudioBuffer;
use ma_core::ids::{NodeId, TrackId};

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

    /// Set to `true` when samples are dropped during recording due to ring buffer overflow.
    /// Checked and reset by the audio callback after processing.
    pub record_overflow: AtomicBool,

    /// Whether this track has an input connection (from InputNode).
    /// Input-enabled tracks receive live audio; non-input tracks receive player output.
    has_input: bool,

    /// Input monitoring — when enabled, live audio passes through even when not recording.
    pub input_monitoring: Arc<AtomicBool>,
}

impl TrackNode {
    pub fn new(
        id: NodeId,
        track_id: TrackId,
        record_producer: Option<rtrb::Producer<f32>>,
    ) -> Self {
        let has_input = record_producer.is_some();
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
            record_overflow: AtomicBool::new(false),
            has_input,
            input_monitoring: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the track ID this node belongs to.
    pub fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Push audio samples to the recording ring buffer in INTERLEAVED order.
    ///
    /// WAV files expect interleaved samples (L0 R0 L1 R1 ...), so we interleave
    /// here at the source. The disk I/O thread writes samples directly to the
    /// WAV file without reordering.
    ///
    /// Uses batch write (`write_chunk_uninit`) for better throughput — one atomic
    /// operation instead of one per sample.
    ///
    /// Returns the number of samples that were dropped due to buffer overflow.
    #[inline]
    fn push_to_record_buffer(&mut self, buffer: &AudioBuffer) -> usize {
        let producer = match &mut self.record_producer {
            Some(p) => p,
            None => return 0,
        };

        let frames = buffer.frames() as usize;
        let channels = buffer.channels();
        let total_samples = frames * channels;

        match producer.write_chunk_uninit(total_samples) {
            Ok(mut chunk) => {
                let (first, second) = chunk.as_mut_slices();
                let mut written = 0;
                // Write interleaved: L0 R0 L1 R1 ...
                for frame in 0..frames {
                    for ch in 0..channels {
                        let sample = buffer.channel(ch)[frame];
                        if written < first.len() {
                            first[written].write(sample);
                        } else {
                            second[written - first.len()].write(sample);
                        }
                        written += 1;
                    }
                }
                // SAFETY: We initialized exactly `total_samples` elements above.
                unsafe { chunk.commit_all() };
                0
            }
            Err(_) => total_samples, // ring buffer full — all samples dropped
        }
    }
}

impl AudioNode for TrackNode {
    fn process(
        &mut self,
        inputs: &[&AudioBuffer],
        outputs: &mut [&mut AudioBuffer],
        context: &ProcessContext,
    ) {
        let output = match outputs.first_mut() {
            Some(o) => o,
            None => return,
        };

        // If muted, output silence
        // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
        if self.mute.load(Ordering::Relaxed) {
            output.clear();
            return;
        }

        // Solo logic: if any track is soloed and this track is NOT soloed, output silence
        // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
        if context.any_solo && !self.solo.load(Ordering::Relaxed) {
            output.clear();
            return;
        }

        let monitoring = self.input_monitoring.load(Ordering::Relaxed);
        let recording =
            self.record_armed.load(Ordering::Relaxed) && self.is_recording.load(Ordering::Relaxed);

        // For input-enabled tracks: only pass live audio when monitoring or recording.
        // For non-input tracks: always copy player output.
        if self.has_input && !monitoring && !recording {
            output.clear();
        } else if let Some(input) = inputs.first() {
            output.copy_from(input);
        } else {
            output.clear();
            return;
        }

        // Push to recording buffer if armed and recording (pre-fader)
        if recording {
            if let Some(input) = inputs.first() {
                let dropped = self.push_to_record_buffer(input);
                if dropped > 0 {
                    self.record_overflow.store(true, Ordering::Relaxed);
                }
            }
        }

        // Apply volume
        // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
        let vol = self.volume.load(Ordering::Relaxed);
        output.apply_gain(vol);

        // Apply pan
        // ORDERING: Relaxed OK — single-value eventual consistency (UI parameter)
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

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
