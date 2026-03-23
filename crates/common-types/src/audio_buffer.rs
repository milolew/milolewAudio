//! Audio buffer types for passing audio data between graph nodes.
//!
//! Buffers use **non-interleaved** (planar) layout internally for better DSP
//! performance: each channel is a contiguous slice of f32 samples, enabling
//! efficient SIMD operations and cache-friendly sequential access.
//!
//! Conversion from/to cpal's interleaved format happens at the graph boundaries
//! (InputNode and OutputNode).

/// Maximum number of frames per audio callback.
/// This is the upper bound for buffer pre-allocation. Typical values: 64, 128, 256, 512, 1024.
pub const MAX_BUFFER_SIZE: usize = 2048;

/// Maximum number of audio channels per buffer.
pub const MAX_CHANNELS: usize = 2;

/// A fixed-capacity, non-interleaved audio buffer.
///
/// Memory layout: `data[channel][frame]`
/// - Channel 0 occupies `data[0..MAX_BUFFER_SIZE]`
/// - Channel 1 occupies `data[MAX_BUFFER_SIZE..2*MAX_BUFFER_SIZE]`
///
/// The buffer is always pre-allocated to maximum size. The `frames` field
/// indicates how many frames are actually valid in the current callback.
///
/// # Real-time safety
/// This struct contains NO heap allocations. It lives on the stack or in
/// pre-allocated Vec slots. All operations are O(1) or O(n) with n = frames.
#[derive(Clone)]
pub struct AudioBuffer {
    data: [f32; MAX_BUFFER_SIZE * MAX_CHANNELS],
    channels: usize,
    frames: u32,
}

impl AudioBuffer {
    /// Create a new silent (zeroed) audio buffer.
    pub fn new(channels: usize, frames: u32) -> Self {
        debug_assert!(channels <= MAX_CHANNELS);
        debug_assert!((frames as usize) <= MAX_BUFFER_SIZE);
        Self {
            data: [0.0; MAX_BUFFER_SIZE * MAX_CHANNELS],
            channels,
            frames,
        }
    }

    /// Create a stereo buffer (the most common case).
    pub fn stereo(frames: u32) -> Self {
        Self::new(2, frames)
    }

    /// Create a mono buffer.
    pub fn mono(frames: u32) -> Self {
        Self::new(1, frames)
    }

    /// Number of channels in this buffer.
    #[inline]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Number of valid frames in this buffer.
    #[inline]
    pub fn frames(&self) -> u32 {
        self.frames
    }

    /// Set the number of valid frames (e.g., when the callback provides fewer frames).
    #[inline]
    pub fn set_frames(&mut self, frames: u32) {
        debug_assert!((frames as usize) <= MAX_BUFFER_SIZE);
        self.frames = frames;
    }

    /// Get an immutable slice of samples for a specific channel.
    #[inline]
    pub fn channel(&self, ch: usize) -> &[f32] {
        debug_assert!(ch < self.channels);
        let start = ch * MAX_BUFFER_SIZE;
        &self.data[start..start + self.frames as usize]
    }

    /// Get a mutable slice of samples for a specific channel.
    #[inline]
    pub fn channel_mut(&mut self, ch: usize) -> &mut [f32] {
        debug_assert!(ch < self.channels);
        let start = ch * MAX_BUFFER_SIZE;
        &mut self.data[start..start + self.frames as usize]
    }

    /// Fill all channels with silence (0.0).
    #[inline]
    pub fn clear(&mut self) {
        for ch in 0..self.channels {
            let start = ch * MAX_BUFFER_SIZE;
            let end = start + self.frames as usize;
            self.data[start..end].fill(0.0);
        }
    }

    /// Copy data from another buffer. If sizes differ, copies the minimum and zeroes the rest.
    #[inline]
    pub fn copy_from(&mut self, other: &AudioBuffer) {
        let channels = self.channels.min(other.channels);
        let frames = self.frames.min(other.frames) as usize;
        for ch in 0..channels {
            let start = ch * MAX_BUFFER_SIZE;
            self.data[start..start + frames]
                .copy_from_slice(&other.data[start..start + frames]);
            // Zero remaining frames if self has more
            if (self.frames as usize) > frames {
                self.data[start + frames..start + self.frames as usize].fill(0.0);
            }
        }
    }

    /// Add (mix) another buffer's samples into this one.
    /// This is the core mixing operation: `self += other`.
    /// Mixes up to the minimum of both buffers' frame counts.
    #[inline]
    pub fn mix_from(&mut self, other: &AudioBuffer) {
        let channels = self.channels.min(other.channels);
        let frames = self.frames.min(other.frames) as usize;
        for ch in 0..channels {
            let start = ch * MAX_BUFFER_SIZE;
            for i in start..start + frames {
                self.data[i] += other.data[i];
            }
        }
    }

    /// Apply a gain factor to all channels.
    #[inline]
    pub fn apply_gain(&mut self, gain: f32) {
        for ch in 0..self.channels {
            let start = ch * MAX_BUFFER_SIZE;
            let end = start + self.frames as usize;
            for sample in &mut self.data[start..end] {
                *sample *= gain;
            }
        }
    }

    /// Apply stereo panning (constant-power pan law).
    /// `pan`: -1.0 = full left, 0.0 = center, 1.0 = full right.
    #[inline]
    pub fn apply_pan(&mut self, pan: f32) {
        if self.channels < 2 {
            return;
        }
        // Constant-power pan: left = cos(θ), right = sin(θ)
        // where θ = (pan + 1) * π/4
        let theta = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let left_gain = theta.cos();
        let right_gain = theta.sin();

        let frames = self.frames as usize;
        let left = &mut self.data[0..frames];
        for sample in left.iter_mut() {
            *sample *= left_gain;
        }
        let right = &mut self.data[MAX_BUFFER_SIZE..MAX_BUFFER_SIZE + frames];
        for sample in right.iter_mut() {
            *sample *= right_gain;
        }
    }

    /// Compute the peak absolute value across all channels.
    /// Returns per-channel peaks as [left, right] (or just [mono] for mono).
    #[inline]
    pub fn peak_levels(&self) -> [f32; MAX_CHANNELS] {
        let mut peaks = [0.0f32; MAX_CHANNELS];
        for (ch, peak) in peaks.iter_mut().enumerate().take(self.channels) {
            let start = ch * MAX_BUFFER_SIZE;
            let end = start + self.frames as usize;
            let mut max = 0.0f32;
            for &sample in &self.data[start..end] {
                let abs = sample.abs();
                if abs > max {
                    max = abs;
                }
            }
            *peak = max;
        }
        peaks
    }

    /// Deinterleave from cpal's interleaved format into this buffer.
    /// Input: `[L0, R0, L1, R1, ...]`
    /// Output: channel 0 = `[L0, L1, ...]`, channel 1 = `[R0, R1, ...]`
    pub fn from_interleaved(&mut self, interleaved: &[f32], channels: usize, frames: u32) {
        debug_assert!(channels <= MAX_CHANNELS);
        debug_assert!((frames as usize) <= MAX_BUFFER_SIZE);
        debug_assert!(interleaved.len() >= channels * frames as usize);
        self.channels = channels;
        self.frames = frames;
        for frame in 0..frames as usize {
            for ch in 0..channels {
                self.data[ch * MAX_BUFFER_SIZE + frame] =
                    interleaved[frame * channels + ch];
            }
        }
    }

    /// Interleave from this buffer into cpal's interleaved format.
    /// Input: channel 0 = `[L0, L1, ...]`, channel 1 = `[R0, R1, ...]`
    /// Output: `[L0, R0, L1, R1, ...]`
    pub fn to_interleaved(&self, output: &mut [f32]) {
        debug_assert!(output.len() >= self.channels * self.frames as usize);
        for frame in 0..self.frames as usize {
            for ch in 0..self.channels {
                output[frame * self.channels + ch] =
                    self.data[ch * MAX_BUFFER_SIZE + frame];
            }
        }
    }

    /// Clamp all samples to [-1.0, 1.0] range.
    /// Call this at the output stage to prevent clipping artifacts in the DAC.
    #[inline]
    pub fn clamp(&mut self) {
        for ch in 0..self.channels {
            let start = ch * MAX_BUFFER_SIZE;
            let end = start + self.frames as usize;
            for sample in &mut self.data[start..end] {
                *sample = sample.clamp(-1.0, 1.0);
            }
        }
    }
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self::stereo(256)
    }
}

impl std::fmt::Debug for AudioBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioBuffer")
            .field("channels", &self.channels)
            .field("frames", &self.frames)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_silent() {
        let buf = AudioBuffer::stereo(256);
        for ch in 0..2 {
            for &sample in buf.channel(ch) {
                assert_eq!(sample, 0.0);
            }
        }
    }

    #[test]
    fn interleave_round_trip() {
        let interleaved: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();
        // [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7] = 4 frames, 2 channels
        let mut buf = AudioBuffer::stereo(4);
        buf.from_interleaved(&interleaved, 2, 4);

        // Channel 0 (left): [0.0, 0.2, 0.4, 0.6]
        assert_eq!(buf.channel(0), &[0.0, 0.2, 0.4, 0.6]);
        // Channel 1 (right): [0.1, 0.3, 0.5, 0.7]
        assert_eq!(buf.channel(1), &[0.1, 0.3, 0.5, 0.7]);

        let mut output = vec![0.0f32; 8];
        buf.to_interleaved(&mut output);
        for (a, b) in interleaved.iter().zip(output.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn mix_adds_buffers() {
        let mut a = AudioBuffer::mono(4);
        a.channel_mut(0).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);

        let mut b = AudioBuffer::mono(4);
        b.channel_mut(0).copy_from_slice(&[0.5, 0.5, 0.5, 0.5]);

        a.mix_from(&b);
        assert_eq!(a.channel(0), &[1.5, 2.5, 3.5, 4.5]);
    }

    #[test]
    fn gain_scales_all_samples() {
        let mut buf = AudioBuffer::stereo(4);
        buf.channel_mut(0).copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
        buf.channel_mut(1).copy_from_slice(&[0.5, 0.5, 0.5, 0.5]);
        buf.apply_gain(0.5);
        assert_eq!(buf.channel(0), &[0.5, 0.5, 0.5, 0.5]);
        assert_eq!(buf.channel(1), &[0.25, 0.25, 0.25, 0.25]);
    }

    #[test]
    fn peak_levels_correct() {
        let mut buf = AudioBuffer::stereo(4);
        buf.channel_mut(0).copy_from_slice(&[0.1, -0.9, 0.5, 0.2]);
        buf.channel_mut(1).copy_from_slice(&[-0.3, 0.7, 0.0, 0.1]);
        let peaks = buf.peak_levels();
        assert!((peaks[0] - 0.9).abs() < 1e-6);
        assert!((peaks[1] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn center_pan_preserves_balance() {
        let mut buf = AudioBuffer::stereo(4);
        buf.channel_mut(0).fill(1.0);
        buf.channel_mut(1).fill(1.0);
        buf.apply_pan(0.0);
        // At center pan, both channels should have equal gain ≈ 0.707
        let expected = std::f32::consts::FRAC_PI_4.cos();
        for &s in buf.channel(0) {
            assert!((s - expected).abs() < 1e-5);
        }
        for &s in buf.channel(1) {
            assert!((s - expected).abs() < 1e-5);
        }
    }
}
