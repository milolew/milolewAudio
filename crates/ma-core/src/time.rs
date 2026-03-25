//! Time representation for the DAW.
//!
//! All time exists in two domains:
//! - **Musical time**: Ticks (960 PPQN — pulses per quarter note)
//! - **Absolute time**: Samples (at project sample rate, e.g., 44100 or 48000)
//!
//! The Transport is the ONLY component that converts between them using current tempo.
//! UI displays Bars:Beats:Ticks. Audio engine works in samples.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors related to time and tempo calculations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimeError {
    /// The time signature has an invalid numerator or denominator (zero).
    #[error("invalid time signature {numerator}/{denominator}: neither may be zero")]
    InvalidTimeSignature { numerator: u8, denominator: u8 },
}

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
#[deprecated(
    since = "0.2.0",
    note = "Use ticks_to_samples() which returns Option<SamplePos>"
)]
pub fn ticks_to_samples_or_zero(tick: Tick, tempo_bpm: f64, sample_rate: f64) -> SamplePos {
    ticks_to_samples(tick, tempo_bpm, sample_rate).unwrap_or(0)
}

/// Convert samples to ticks, returning 0 on invalid input.
///
/// This is a backwards-compatible wrapper around [`samples_to_ticks`].
#[deprecated(
    since = "0.2.0",
    note = "Use samples_to_ticks() which returns Option<Tick>"
)]
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
    /// Returns `Err(TimeError::InvalidTimeSignature)` if `numerator` or `denominator` is zero.
    ///
    /// # Arguments
    /// * `absolute_tick` - Position in ticks from project start
    /// * `numerator` - Time signature numerator (e.g., 4 for 4/4), must be > 0
    /// * `denominator` - Time signature denominator (e.g., 4 for 4/4), must be > 0
    pub fn from_ticks(
        absolute_tick: Tick,
        numerator: u8,
        denominator: u8,
    ) -> Result<Self, TimeError> {
        if numerator == 0 || denominator == 0 {
            return Err(TimeError::InvalidTimeSignature {
                numerator,
                denominator,
            });
        }
        let denominator = denominator as Tick;
        let numerator = numerator as Tick;
        let ticks_per_beat = PPQN * 4 / denominator;
        let ticks_per_bar = ticks_per_beat * numerator;

        let tick = absolute_tick.max(0);
        let bar = (tick / ticks_per_bar) as u32 + 1; // 1-indexed
        let remaining = tick % ticks_per_bar;
        let beat = (remaining / ticks_per_beat) as u8 + 1; // 1-indexed
        let sub_tick = (remaining % ticks_per_beat) as u16;

        Ok(Self {
            bar,
            beat,
            tick: sub_tick,
        })
    }

    /// Convert from absolute ticks, clamping invalid time signature values to 1.
    ///
    /// This is a backwards-compatible wrapper around [`BarBeatTick::from_ticks`].
    #[deprecated(
        since = "0.2.0",
        note = "Use from_ticks() which returns Result<BarBeatTick, TimeError>"
    )]
    pub fn from_ticks_clamped(absolute_tick: Tick, numerator: u8, denominator: u8) -> Self {
        Self::from_ticks(absolute_tick, numerator.max(1), denominator.max(1))
            .expect("clamped values are always valid")
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
        let bbt = BarBeatTick::from_ticks(0, 4, 4).unwrap();
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 1,
                beat: 1,
                tick: 0
            }
        );

        // Tick 960 = Bar 1, Beat 2, Tick 0
        let bbt = BarBeatTick::from_ticks(960, 4, 4).unwrap();
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 1,
                beat: 2,
                tick: 0
            }
        );

        // Tick 3840 = Bar 2, Beat 1, Tick 0 (4 beats × 960 = 3840)
        let bbt = BarBeatTick::from_ticks(3840, 4, 4).unwrap();
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
    fn bar_beat_tick_zero_denominator_returns_error() {
        let result = BarBeatTick::from_ticks(0, 4, 0);
        assert_eq!(
            result,
            Err(TimeError::InvalidTimeSignature {
                numerator: 4,
                denominator: 0
            })
        );
    }

    #[test]
    fn bar_beat_tick_zero_numerator_returns_error() {
        let result = BarBeatTick::from_ticks(0, 0, 4);
        assert_eq!(
            result,
            Err(TimeError::InvalidTimeSignature {
                numerator: 0,
                denominator: 4
            })
        );
    }

    #[test]
    fn bar_beat_tick_both_zero_returns_error() {
        let result = BarBeatTick::from_ticks(0, 0, 0);
        assert_eq!(
            result,
            Err(TimeError::InvalidTimeSignature {
                numerator: 0,
                denominator: 0
            })
        );
    }

    #[test]
    fn bar_beat_tick_3_4_time() {
        // In 3/4: bar = 3 beats × 960 ticks = 2880 ticks
        let bbt = BarBeatTick::from_ticks(2880, 3, 4).unwrap();
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 2,
                beat: 1,
                tick: 0
            }
        );
    }

    // ── C6: Edge-case tests for time ────────────────────────────────

    #[test]
    fn negative_ticks_clamped_to_zero_in_bbt() {
        // Negative ticks should be clamped to 0, yielding bar 1, beat 1, tick 0.
        let bbt = BarBeatTick::from_ticks(-100, 4, 4).unwrap();
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 1,
                beat: 1,
                tick: 0
            }
        );
    }

    #[test]
    fn negative_ticks_large_value_clamped() {
        let bbt = BarBeatTick::from_ticks(i64::MIN, 4, 4).unwrap();
        assert_eq!(
            bbt,
            BarBeatTick {
                bar: 1,
                beat: 1,
                tick: 0
            }
        );
    }

    #[test]
    fn ticks_to_samples_negative_tick() {
        // Negative ticks should produce a negative sample position.
        let result = ticks_to_samples(-960, 120.0, 48000.0).unwrap();
        assert_eq!(result, -24000);
    }

    #[test]
    fn ticks_to_samples_zero_tick() {
        let result = ticks_to_samples(0, 120.0, 48000.0).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn samples_to_ticks_zero_sample() {
        let result = samples_to_ticks(0, 120.0, 48000.0).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn samples_to_ticks_negative_sample() {
        let result = samples_to_ticks(-24000, 120.0, 48000.0).unwrap();
        assert_eq!(result, -960);
    }

    #[test]
    fn ticks_to_samples_very_fast_tempo() {
        // At 300 BPM: 1 quarter note = 60/300 = 0.2 seconds = 9600 samples at 48kHz
        let result = ticks_to_samples(960, 300.0, 48000.0).unwrap();
        assert_eq!(result, 9600);
    }

    #[test]
    fn ticks_to_samples_very_slow_tempo() {
        // At 20 BPM: 1 quarter note = 60/20 = 3.0 seconds = 144000 samples at 48kHz
        let result = ticks_to_samples(960, 20.0, 48000.0).unwrap();
        assert_eq!(result, 144000);
    }

    #[test]
    fn ticks_to_samples_high_sample_rate() {
        // At 120 BPM, 96kHz: 960 ticks = 0.5 seconds = 48000 samples
        let result = ticks_to_samples(960, 120.0, 96000.0).unwrap();
        assert_eq!(result, 48000);
    }

    #[test]
    fn round_trip_accuracy_at_various_tempos() {
        let tempos = [60.0, 90.0, 120.0, 140.0, 160.0, 200.0, 300.0];
        let rates = [44100.0, 48000.0, 96000.0];
        let tick_values = [0i64, 1, 480, 960, 1920, 3840, 100_000];

        for &tempo in &tempos {
            for &rate in &rates {
                for &tick in &tick_values {
                    let samples = ticks_to_samples(tick, tempo, rate).unwrap();
                    let back = samples_to_ticks(samples, tempo, rate).unwrap();
                    assert!(
                        (back - tick).abs() <= 1,
                        "Round trip drift > 1 at tempo={}, rate={}, tick={}: got {}",
                        tempo,
                        rate,
                        tick,
                        back
                    );
                }
            }
        }
    }

    #[test]
    fn large_tick_value() {
        // Very large tick value (roughly 1 million quarter notes).
        let large_tick: Tick = 960_000_000;
        let result = ticks_to_samples(large_tick, 120.0, 48000.0);
        assert!(result.is_some());
        let samples = result.unwrap();
        // 960_000_000 ticks / 960 = 1_000_000 quarter notes
        // At 120 BPM = 500_000 minutes = 30_000_000 seconds
        // At 48kHz = 1_440_000_000_000 samples
        assert!(samples > 0);
    }

    #[test]
    fn very_large_tick_in_bbt() {
        // Test with a large tick value (many bars).
        let large_tick: Tick = 3840 * 1000; // 1000 bars in 4/4
        let bbt = BarBeatTick::from_ticks(large_tick, 4, 4).unwrap();
        assert_eq!(bbt.bar, 1001); // 1-indexed
        assert_eq!(bbt.beat, 1);
        assert_eq!(bbt.tick, 0);
    }

    #[test]
    fn bbt_with_sub_ticks() {
        // Tick 500 in 4/4: should be bar 1, beat 1, tick 500.
        let bbt = BarBeatTick::from_ticks(500, 4, 4).unwrap();
        assert_eq!(bbt.bar, 1);
        assert_eq!(bbt.beat, 1);
        assert_eq!(bbt.tick, 500);
    }

    #[test]
    fn bbt_6_8_time() {
        // 6/8 time: 6 beats of eighth notes.
        // Ticks per beat (eighth note) = 960 * 4 / 8 = 480.
        // Ticks per bar = 480 * 6 = 2880.
        let bbt = BarBeatTick::from_ticks(2880, 6, 8).unwrap();
        assert_eq!(bbt.bar, 2);
        assert_eq!(bbt.beat, 1);
        assert_eq!(bbt.tick, 0);
    }

    #[test]
    fn bbt_7_8_time() {
        // 7/8 time: 7 beats of eighth notes.
        // Ticks per beat = 480.
        // Ticks per bar = 480 * 7 = 3360.
        let bbt = BarBeatTick::from_ticks(3360, 7, 8).unwrap();
        assert_eq!(bbt.bar, 2);
        assert_eq!(bbt.beat, 1);
        assert_eq!(bbt.tick, 0);
    }

    #[test]
    fn bbt_serialization_round_trip() {
        let bbt = BarBeatTick {
            bar: 42,
            beat: 3,
            tick: 480,
        };
        let json = serde_json::to_string(&bbt).unwrap();
        let deserialized: BarBeatTick = serde_json::from_str(&json).unwrap();
        assert_eq!(bbt, deserialized);
    }

    #[test]
    fn time_error_display() {
        let err = TimeError::InvalidTimeSignature {
            numerator: 0,
            denominator: 4,
        };
        assert_eq!(
            err.to_string(),
            "invalid time signature 0/4: neither may be zero"
        );
    }

    #[test]
    fn negative_sample_rate_returns_none() {
        assert_eq!(ticks_to_samples(960, 120.0, -48000.0), None);
        assert_eq!(samples_to_ticks(24000, 120.0, -48000.0), None);
    }

    #[test]
    fn very_small_positive_tempo() {
        // Very small but positive tempo should still work.
        let result = ticks_to_samples(960, 0.001, 48000.0);
        assert!(result.is_some());
        let samples = result.unwrap();
        assert!(samples > 0);
    }
}
