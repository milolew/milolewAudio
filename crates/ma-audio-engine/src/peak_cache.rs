//! Peak cache — hierarchical mipmap for efficient waveform rendering.
//!
//! Stores pre-computed min/max pairs at multiple block sizes (64, 256, 1024, 4096).
//! The arrangement view selects the appropriate level based on zoom, so waveform
//! rendering is always O(pixels) regardless of clip length.

/// Block sizes for each mipmap level.
const BLOCK_SIZES: [usize; 4] = [64, 256, 1024, 4096];

/// Hierarchical peak cache for waveform rendering.
pub struct PeakCache {
    /// Mipmap levels, one per block size.
    pub levels: Vec<PeakLevel>,
    /// Number of audio channels.
    pub channels: usize,
    /// Total samples per channel in the source audio.
    pub total_samples: usize,
}

/// A single mipmap level containing min/max pairs at a fixed block size.
pub struct PeakLevel {
    /// Number of samples summarized per (min, max) pair.
    pub block_size: usize,
    /// Per-channel peak data: `peaks[channel][block_index] = (min, max)`.
    pub peaks: Vec<Vec<(f32, f32)>>,
}

/// Build a hierarchical peak cache from non-interleaved audio samples.
///
/// # Arguments
/// * `samples` — non-interleaved: ch0 at `[0..length]`, ch1 at `[length..2*length]`
/// * `channels` — number of audio channels
/// * `length` — number of samples per channel
pub fn build_peak_cache(samples: &[f32], channels: usize, length: usize) -> PeakCache {
    let levels = BLOCK_SIZES
        .iter()
        .map(|&block_size| build_level(samples, channels, length, block_size))
        .collect();

    PeakCache {
        levels,
        channels,
        total_samples: length,
    }
}

/// Build a single mipmap level.
fn build_level(samples: &[f32], channels: usize, length: usize, block_size: usize) -> PeakLevel {
    let num_blocks = length.div_ceil(block_size);
    let mut peaks = Vec::with_capacity(channels);

    for ch in 0..channels {
        let ch_offset = ch * length;
        let mut ch_peaks = Vec::with_capacity(num_blocks);

        for block in 0..num_blocks {
            let start = ch_offset + block * block_size;
            let end = (ch_offset + (block + 1) * block_size).min(ch_offset + length);

            if start >= samples.len() || start >= end {
                ch_peaks.push((0.0, 0.0));
                continue;
            }

            let actual_end = end.min(samples.len());
            let slice = &samples[start..actual_end];

            let mut min_val = f32::MAX;
            let mut max_val = f32::MIN;
            for &s in slice {
                if s < min_val {
                    min_val = s;
                }
                if s > max_val {
                    max_val = s;
                }
            }

            ch_peaks.push((min_val, max_val));
        }

        peaks.push(ch_peaks);
    }

    PeakLevel { block_size, peaks }
}

impl PeakCache {
    /// Get peak data for a sample range, targeting a specific pixel resolution.
    ///
    /// Selects the mipmap level where `block_size` is closest to `samples_per_pixel`.
    /// Returns `(min, max)` pairs — one per output pixel.
    ///
    /// # Arguments
    /// * `channel` — which channel to read
    /// * `start_sample` — first sample in the visible range
    /// * `end_sample` — last sample in the visible range (exclusive)
    /// * `target_pixels` — how many pixel columns to fill
    pub fn peaks_for_range(
        &self,
        channel: usize,
        start_sample: usize,
        end_sample: usize,
        target_pixels: usize,
    ) -> Vec<(f32, f32)> {
        if target_pixels == 0 || channel >= self.channels || start_sample >= end_sample {
            return Vec::new();
        }

        let range_samples = end_sample.saturating_sub(start_sample);
        let samples_per_pixel = range_samples / target_pixels.max(1);

        // Pick the best mipmap level
        let level = self
            .levels
            .iter()
            .rev()
            .find(|l| l.block_size <= samples_per_pixel.max(1))
            .unwrap_or(&self.levels[0]);

        let ch_peaks = match level.peaks.get(channel) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut result = Vec::with_capacity(target_pixels);

        for pixel in 0..target_pixels {
            let pixel_start = start_sample + (range_samples * pixel) / target_pixels;
            let pixel_end = start_sample + (range_samples * (pixel + 1)) / target_pixels;

            let block_start = pixel_start / level.block_size;
            let block_end = pixel_end.div_ceil(level.block_size).min(ch_peaks.len());

            let mut min_val = f32::MAX;
            let mut max_val = f32::MIN;
            let mut found = false;

            for block_idx in block_start..block_end {
                if let Some(&(bmin, bmax)) = ch_peaks.get(block_idx) {
                    if bmin < min_val {
                        min_val = bmin;
                    }
                    if bmax > max_val {
                        max_val = bmax;
                    }
                    found = true;
                }
            }

            if found {
                result.push((min_val, max_val));
            } else {
                result.push((0.0, 0.0));
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a mono sine wave for testing.
    fn sine_wave(length: usize, freq: f32, sample_rate: f32) -> Vec<f32> {
        (0..length)
            .map(|i| (i as f32 / sample_rate * freq * std::f32::consts::TAU).sin())
            .collect()
    }

    #[test]
    fn build_cache_mono() {
        let samples = sine_wave(4800, 440.0, 48000.0);
        let cache = build_peak_cache(&samples, 1, 4800);

        assert_eq!(cache.channels, 1);
        assert_eq!(cache.total_samples, 4800);
        assert_eq!(cache.levels.len(), 4);
        assert_eq!(cache.levels[0].block_size, 64);
        assert_eq!(cache.levels[3].block_size, 4096);

        // Level 0 (block=64): 4800/64 = 75 blocks
        assert_eq!(cache.levels[0].peaks[0].len(), 75);
    }

    #[test]
    fn build_cache_stereo() {
        let length = 2400;
        let mut samples = sine_wave(length, 440.0, 48000.0);
        // Append channel 1 (same data)
        let ch1 = samples.clone();
        samples.extend_from_slice(&ch1);

        let cache = build_peak_cache(&samples, 2, length);

        assert_eq!(cache.channels, 2);
        assert_eq!(cache.levels[0].peaks.len(), 2);

        // Both channels should have the same peaks
        for (a, b) in cache.levels[0].peaks[0]
            .iter()
            .zip(cache.levels[0].peaks[1].iter())
        {
            assert!((a.0 - b.0).abs() < f32::EPSILON);
            assert!((a.1 - b.1).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn peak_values_correct() {
        // Create known signal: [0.5, -0.3, 0.8, -0.1] repeated
        let samples: Vec<f32> = (0..256)
            .map(|i| match i % 4 {
                0 => 0.5,
                1 => -0.3,
                2 => 0.8,
                3 => -0.1,
                _ => unreachable!(),
            })
            .collect();

        let cache = build_peak_cache(&samples, 1, 256);

        // Level 0 (block=64): 256/64 = 4 blocks
        let level0 = &cache.levels[0];
        assert_eq!(level0.peaks[0].len(), 4);

        // Each 64-sample block contains the same pattern
        for (min, max) in &level0.peaks[0] {
            assert!((*min - (-0.3)).abs() < f32::EPSILON, "min should be -0.3");
            assert!((*max - 0.8).abs() < f32::EPSILON, "max should be 0.8");
        }
    }

    #[test]
    fn peaks_for_range_basic() {
        let samples = sine_wave(48000, 440.0, 48000.0);
        let cache = build_peak_cache(&samples, 1, 48000);

        let peaks = cache.peaks_for_range(0, 0, 48000, 100);
        assert_eq!(peaks.len(), 100);

        // Each pixel should span ~480 samples of a 440Hz sine → full amplitude cycle
        for &(min, max) in &peaks {
            assert!(min < 0.0, "Sine wave should have negative peaks");
            assert!(max > 0.0, "Sine wave should have positive peaks");
        }
    }

    #[test]
    fn peaks_for_range_empty() {
        let cache = build_peak_cache(&[], 1, 0);
        let peaks = cache.peaks_for_range(0, 0, 0, 100);
        assert!(peaks.is_empty());
    }

    #[test]
    fn peaks_for_range_out_of_channel() {
        let samples = sine_wave(1000, 440.0, 48000.0);
        let cache = build_peak_cache(&samples, 1, 1000);
        let peaks = cache.peaks_for_range(5, 0, 1000, 10);
        assert!(peaks.is_empty());
    }
}
