//! Time representation for the DAW.
//!
//! All time exists in two domains:
//! - **Musical time**: Ticks (960 PPQN — pulses per quarter note)
//! - **Absolute time**: Samples (at project sample rate, e.g., 44100 or 48000)
//!
//! The Transport is the ONLY component that converts between them using current tempo.
//! UI displays Bars:Beats:Ticks. Audio engine works in samples.

use serde::{Deserialize, Serialize};

/// Musical time unit: pulses (ticks) relative to the project start.
/// 960 ticks = 1 quarter note at any tempo.
pub type Tick = i64;

/// Absolute sample position in the audio stream.
/// Signed to allow negative positions (pre-roll, count-in).
pub type SamplePos = i64;

/// Number of frames in a single audio callback buffer.
/// Unsigned because buffer sizes are always non-negative.
pub type FrameCount = u32;

/// Pulses Per Quarter Note — the resolution of musical time.
/// 960 is a common DAW standard (divisible by 2, 3, 4, 5, 6, 8, 10, 12, 15, 16...).
pub const PPQN: Tick = 960;

/// Convert a tick position to a sample position given tempo and sample rate.
///
/// Returns `None` when `tempo_bpm <= 0.0` or `sample_rate <= 0.0`.
///
/// Formula: samples = ticks * 60.0 * sample_rate / (tempo_bpm * PPQN)
///
/// # Arguments
/// * `tick` - Position in musical ticks
/// * `tempo_bpm` - Tempo in beats per minute (must be > 0.0)
/// * `sample_rate` - Audio sample rate in Hz (must be > 0.0)
pub fn ticks_to_samples(tick: Tick, tempo_bpm: f64, sample_rate: f64) -> Option<SamplePos> {
    if tempo_bpm <= 0.0 || sample_rate <= 0.0 {
        return None;
    }
    let seconds_per_tick = 60.0 / (tempo_bpm * PPQN as f64);
    Some((tick as f64 * seconds_per_tick * sample_rate) as SamplePos)
}

/// Convert a sample position to ticks given tempo and sample rate.
///
/// Returns `None` when `tempo_bpm <= 0.0` or `sample_rate <= 0.0`.
///
/// Formula: ticks = samples * tempo_bpm * PPQN / (60.0 * sample_rate)
///
/// # Arguments
/// * `sample` - Position in samples
/// * `tempo_bpm` - Tempo in beats per minute (must be > 0.0)
/// * `sample_rate` - Audio sample rate in Hz (must be > 0.0)
pub fn samples_to_ticks(sample: SamplePos, tempo_bpm: f64, sample_rate: f64) -> Option<Tick> {
    if tempo_bpm <= 0.0 || sample_rate <= 0.0 {
        return None;
    }
    let ticks_per_sample = (tempo_bpm * PPQN as f64) / (60.0 * sample_rate);
    Some((sample as f64 * ticks_per_sample) as Tick)
}

/// Convert ticks to samples, returning 0 on invalid input.
///
/// This is a backwards-compatible wrapper around [`ticks_to_samples`].
#[deprecated(since = "0.2.0", note = "Use ticks_to_samples() which returns Option<SamplePos>")]
pub fn ticks_to_samples_or_zero(tick: Tick, tempo_bpm: f64, sample_rate: f64) -> SamplePos {
    ticks_to_samples(tick, tempo_bpm, sample_rate).unwrap_or(0)
}

/// Convert samples to ticks, returning 0 on invalid input.
///
/// This is a backwards-compatible wrapper around [`samples_to_ticks`].
#[deprecated(since = "0.2.0", note = "Use samples_to_ticks() which returns Option<Tick>")]
pub fn samples_to_ticks_or_zero(sample: SamplePos, tempo_bpm: f64, sample_rate: f64) -> Tick {
    samples_to_ticks(sample, tempo_bpm, sample_rate).unwrap_or(0)
}

/// Display-friendly representation of a musical position: Bars:Beats:Ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarBeatTick {
    pub bar: u32,
    pub beat: u8,
    pub tick: u16,
}

impl BarBeatTick {
    /// Convert from absolute ticks to Bars:Beats:Ticks display format.
    ///
    /// # Arguments
    /// * `absolute_tick` - Position in ticks from project start
    /// * `numerator` - Time signature numerator (e.g., 4 for 4/4)
    /// * `denominator` - Time signature denominator (e.g., 4 for 4/4)
    pub fn from_ticks(absolute_tick: Tick, numerator: u8, denominator: u8) -> Self {
        let denominator = (denominator as Tick).max(1);
        let numerator = (numerator as Tick).max(1);
        let ticks_per_beat = PPQN * 4 / denominator;
        let ticks_per_bar = ticks_per_beat * numerator;

        let tick = absolute_tick.max(0);
        let bar = (tick / ticks_per_bar) as u32 + 1; // 1-indexed
        let remaining = tick % ticks_per_bar;
        let beat = (remaining / ticks_per_beat) as u8 + 1; // 1-indexed
        let sub_tick = (remaining % ticks_per_beat) as u16;

        Self {
            bar,
            beat,
            tick: sub_tick,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_to_sample_conversion_at_120bpm() {
        // At 120 BPM, 48000 Hz:
        // 1 quarter note = 0.5 seconds = 24000 samples
        // 1 quarter note = 960 ticks
        // So 960 ticks = 24000 samples
        let samples = ticks_to_samples(960, 120.0, 48000.0).unwrap();
        assert_eq!(samples, 24000);
    }

    #[test]
    fn sample_to_tick_conversion_at_120bpm() {
        let ticks = samples_to_ticks(24000, 120.0, 48000.0).unwrap();
        assert_eq!(ticks, 960);
    }

    #[test]
    fn round_trip_conversion() {
        let original_tick: Tick = 1920; // 2 quarter notes
        let samples = ticks_to_samples(original_tick, 140.0, 44100.0).unwrap();
        let back = samples_to_ticks(samples, 140.0, 44100.0).unwrap();
        assert!(
            (back - original_tick).abs() <= 1,
            "Round trip drift > 1 tick"
        );
    }

    #[test]
    fn bar_beat_tick_4_4_time() {
        // Tick 0 = Bar 1, Beat 1, Tick 0
        let bbt = BarBeatTick::from_ticks(0, 4, 4);
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 1,
                beat: 1,
                tick: 0
            }
        );

        // Tick 960 = Bar 1, Beat 2, Tick 0
        let bbt = BarBeatTick::from_ticks(960, 4, 4);
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 1,
                beat: 2,
                tick: 0
            }
        );

        // Tick 3840 = Bar 2, Beat 1, Tick 0 (4 beats × 960 = 3840)
        let bbt = BarBeatTick::from_ticks(3840, 4, 4);
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 2,
                beat: 1,
                tick: 0
            }
        );
    }

    #[test]
    fn zero_tempo_returns_none() {
        assert_eq!(ticks_to_samples(960, 0.0, 48000.0), None);
        assert_eq!(samples_to_ticks(24000, 0.0, 48000.0), None);
    }

    #[test]
    fn zero_sample_rate_returns_none() {
        assert_eq!(ticks_to_samples(960, 120.0, 0.0), None);
        assert_eq!(samples_to_ticks(24000, 120.0, 0.0), None);
    }

    #[test]
    fn negative_tempo_returns_none() {
        assert_eq!(ticks_to_samples(960, -120.0, 48000.0), None);
        assert_eq!(samples_to_ticks(24000, -120.0, 48000.0), None);
    }

    #[test]
    fn bar_beat_tick_zero_denominator_clamped() {
        // Should not panic — denominator clamped to 1
        let bbt = BarBeatTick::from_ticks(0, 4, 0);
        assert_eq!(bbt.bar, 1);
    }

    #[test]
    fn bar_beat_tick_zero_numerator_clamped() {
        // Should not panic — numerator clamped to 1
        let bbt = BarBeatTick::from_ticks(0, 0, 4);
        assert_eq!(bbt.bar, 1);
    }

    #[test]
    fn bar_beat_tick_3_4_time() {
        // In 3/4: bar = 3 beats × 960 ticks = 2880 ticks
        let bbt = BarBeatTick::from_ticks(2880, 3, 4);
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 2,
                beat: 1,
                tick: 0
            }
        );
    }
}
