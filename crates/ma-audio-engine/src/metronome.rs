//! Metronome — generates click audio for count-in and playback.
//!
//! Uses sine wave synthesis with stack-only processing.
//! No heap allocation in `next_sample()` — safe for the audio thread.

/// Click duration in seconds.
const CLICK_DURATION_SECS: f64 = 0.02;

/// Fraction of the click used for fade-out envelope.
const FADE_FRACTION: f64 = 0.25;

/// Metronome click generator.
///
/// Produces short sine wave bursts: 880 Hz for downbeats, 440 Hz for upbeats.
/// The click includes a fade-out envelope for the last 25% of its duration
/// to avoid harsh pops.
pub struct Metronome {
    /// Oscillator phase (0.0–1.0 cycle).
    phase: f64,
    /// Remaining samples in the current click (0 = silent).
    click_remaining: u32,
    /// Total samples in a click (for envelope calculation).
    click_total: u32,
    /// Current click frequency in Hz.
    frequency: f64,
    /// Audio sample rate.
    sample_rate: f64,
    /// Click amplitude (linear gain).
    amplitude: f32,
}

impl Metronome {
    /// Create a new metronome for the given sample rate.
    pub fn new(sample_rate: f64) -> Self {
        let click_total = (sample_rate * CLICK_DURATION_SECS) as u32;
        Self {
            phase: 0.0,
            click_remaining: 0,
            click_total,
            frequency: 880.0,
            sample_rate,
            amplitude: 0.3,
        }
    }

    /// Trigger a click sound.
    ///
    /// `downbeat = true` produces a higher-pitched accent (880 Hz).
    /// `downbeat = false` produces a regular beat (440 Hz).
    pub fn trigger(&mut self, downbeat: bool) {
        self.frequency = if downbeat { 880.0 } else { 440.0 };
        self.click_remaining = self.click_total;
        self.phase = 0.0;
    }

    /// Generate one sample of click audio. Returns 0.0 when not clicking.
    ///
    /// # Real-Time Safety
    /// Stack-only computation, no allocations.
    #[inline]
    pub fn next_sample(&mut self) -> f32 {
        if self.click_remaining == 0 {
            return 0.0;
        }
        self.click_remaining -= 1;

        // Sine oscillator
        let sample = (self.phase * std::f64::consts::TAU).sin() as f32 * self.amplitude;
        self.phase += self.frequency / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // Fade-out envelope for the last FADE_FRACTION of the click
        let fade_threshold = (self.click_total as f64 * FADE_FRACTION) as u32;
        if self.click_remaining < fade_threshold {
            let fade = self.click_remaining as f32 / fade_threshold.max(1) as f32;
            return sample * fade;
        }

        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_when_not_triggered() {
        let mut met = Metronome::new(48000.0);
        for _ in 0..1000 {
            assert_eq!(met.next_sample(), 0.0);
        }
    }

    #[test]
    fn produces_audio_after_trigger() {
        let mut met = Metronome::new(48000.0);
        met.trigger(true);
        let mut has_nonzero = false;
        for _ in 0..960 {
            if met.next_sample() != 0.0 {
                has_nonzero = true;
            }
        }
        assert!(has_nonzero);
    }

    #[test]
    fn returns_to_silence_after_click_duration() {
        let mut met = Metronome::new(48000.0);
        met.trigger(false);
        // Click is 20ms = 960 samples at 48kHz
        for _ in 0..960 {
            met.next_sample();
        }
        assert_eq!(met.next_sample(), 0.0);
    }

    #[test]
    fn downbeat_uses_higher_frequency() {
        let mut met = Metronome::new(48000.0);

        met.trigger(true);
        met.next_sample(); // advance
        let freq_high = met.frequency;

        met.trigger(false);
        met.next_sample();
        let freq_low = met.frequency;

        assert!(freq_high > freq_low);
    }
}
