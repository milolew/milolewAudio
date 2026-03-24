//! Input capture — separates cpal input stream state from the output callback.
//!
//! cpal creates separate input and output streams that each need their own
//! owned state. This module provides:
//! - `InputCaptureState` — owned by the cpal INPUT stream closure, pushes samples to ring buffer
//! - `InputCaptureReader` — owned by `CallbackState` (OUTPUT closure), drains samples before graph processing

use std::sync::atomic::{AtomicU64, Ordering};

/// Ring buffer size for input capture (samples, not frames).
/// Sized for 48kHz stereo with 256-frame buffer: 256 * 2 = 512 min.
/// Use 32x headroom for scheduling jitter between input/output callbacks.
pub const INPUT_CAPTURE_RING_SIZE: usize = 16384;

/// State owned by the cpal INPUT stream closure.
///
/// Captures interleaved audio and pushes it into a lock-free ring buffer
/// consumed by the output callback via `InputCaptureReader`.
pub struct InputCaptureState {
    producer: rtrb::Producer<f32>,
    channel_count: usize,
    /// Number of samples dropped due to ring buffer overflow.
    overflow_samples: AtomicU64,
}

impl InputCaptureState {
    pub fn new(producer: rtrb::Producer<f32>, channel_count: usize) -> Self {
        Self {
            producer,
            channel_count,
            overflow_samples: AtomicU64::new(0),
        }
    }

    /// Called by cpal input callback. Pushes raw interleaved samples.
    /// If the ring buffer is full, excess samples are dropped and counted
    /// in `overflow_samples`.
    ///
    /// # Real-Time Safety
    /// This function is lock-free and allocation-free. Uses batch write for cache efficiency.
    #[inline]
    pub fn capture(&mut self, input: &[f32]) {
        let available = self.producer.slots();
        let to_write = input.len().min(available);
        let dropped = input.len() - to_write;
        if dropped > 0 {
            // ORDERING: Relaxed OK — single-value counter, eventual consistency
            self.overflow_samples
                .fetch_add(dropped as u64, Ordering::Relaxed);
        }
        if to_write == 0 {
            return;
        }
        if let Ok(mut chunk) = self.producer.write_chunk_uninit(to_write) {
            let (first, second) = chunk.as_mut_slices();
            let first_len = first.len();
            for (dst, &src) in first.iter_mut().zip(&input[..first_len]) {
                dst.write(src);
            }
            if !second.is_empty() {
                for (dst, &src) in second.iter_mut().zip(&input[first_len..to_write]) {
                    dst.write(src);
                }
            }
            unsafe { chunk.commit_all() };
        }
    }

    pub fn channel_count(&self) -> usize {
        self.channel_count
    }

    /// Take the current overflow sample count, resetting it to zero.
    pub fn take_overflow_samples(&self) -> u64 {
        self.overflow_samples.swap(0, Ordering::Relaxed)
    }
}

/// Reader side, owned by the output callback (inside `CallbackState`).
///
/// Drains captured samples into the InputNode before graph processing.
/// Pre-allocates a staging buffer at construction to avoid RT allocations.
pub struct InputCaptureReader {
    consumer: rtrb::Consumer<f32>,
    channel_count: usize,
    /// Pre-allocated staging buffer to batch ring buffer reads.
    staging_buffer: Vec<f32>,
}

impl InputCaptureReader {
    /// Create a new reader with a pre-allocated staging buffer.
    ///
    /// # Arguments
    /// * `consumer` - Ring buffer consumer side
    /// * `channel_count` - Number of input channels
    /// * `max_buffer_size` - Maximum expected buffer size in frames
    pub fn new(consumer: rtrb::Consumer<f32>, channel_count: usize, max_buffer_size: u32) -> Self {
        let capacity = channel_count * max_buffer_size as usize;
        Self {
            consumer,
            channel_count,
            staging_buffer: vec![0.0; capacity],
        }
    }

    /// Drain available samples from the ring buffer into the staging buffer.
    ///
    /// Returns interleaved samples (up to `expected_frames * channel_count`).
    /// If fewer samples are available, the remainder is zero-filled.
    ///
    /// # Real-Time Safety
    /// Lock-free, no allocations (staging buffer is pre-allocated). Uses batch read for cache efficiency.
    #[inline]
    pub fn drain_into_staging(&mut self, expected_frames: u32) -> &[f32] {
        let expected_samples = expected_frames as usize * self.channel_count;
        let len = expected_samples.min(self.staging_buffer.len());
        let available = self.consumer.slots();
        let to_read = len.min(available);

        let count = if to_read > 0 {
            if let Ok(chunk) = self.consumer.read_chunk(to_read) {
                let (first, second) = chunk.as_slices();
                let first_len = first.len();
                let second_len = second.len();
                self.staging_buffer[..first_len].copy_from_slice(first);
                if second_len > 0 {
                    self.staging_buffer[first_len..first_len + second_len].copy_from_slice(second);
                }
                let total = first_len + second_len;
                chunk.commit_all();
                total
            } else {
                0
            }
        } else {
            0
        };

        // Zero-fill any remaining samples
        for sample in &mut self.staging_buffer[count..len] {
            *sample = 0.0;
        }

        &self.staging_buffer[..len]
    }

    pub fn channel_count(&self) -> usize {
        self.channel_count
    }
}

/// Create a matched pair of input capture state and reader.
///
/// # Arguments
/// * `channel_count` - Number of input channels (typically 2 for stereo)
/// * `max_buffer_size` - Maximum expected buffer size in frames
pub fn create_input_capture(
    channel_count: usize,
    max_buffer_size: u32,
) -> (InputCaptureState, InputCaptureReader) {
    let (producer, consumer) = rtrb::RingBuffer::new(INPUT_CAPTURE_RING_SIZE);
    let state = InputCaptureState::new(producer, channel_count);
    let reader = InputCaptureReader::new(consumer, channel_count, max_buffer_size);
    (state, reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_and_drain_round_trip() {
        let (mut capture, mut reader) = create_input_capture(2, 256);

        // Simulate input callback pushing 4 frames of stereo audio
        let input: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();
        capture.capture(&input);

        // Simulate output callback draining
        let output = reader.drain_into_staging(4);
        assert_eq!(output.len(), 8);
        for i in 0..8 {
            assert!((output[i] - i as f32 * 0.1).abs() < 1e-6);
        }
    }

    #[test]
    fn drain_with_insufficient_data_zero_fills() {
        let (_capture, mut reader) = create_input_capture(2, 256);

        // Drain without any capture — should get zeros
        let output = reader.drain_into_staging(4);
        assert_eq!(output.len(), 8);
        for &sample in output {
            assert_eq!(sample, 0.0);
        }
    }

    #[test]
    fn capture_overflow_drops_silently() {
        let (mut capture, mut reader) = create_input_capture(1, 256);

        // Push more samples than the ring buffer can hold
        let huge_input: Vec<f32> = vec![1.0; INPUT_CAPTURE_RING_SIZE + 100];
        capture.capture(&huge_input);

        // Should drain up to ring buffer capacity, no panic
        let output = reader.drain_into_staging(256);
        assert_eq!(output.len(), 256);
    }
}
