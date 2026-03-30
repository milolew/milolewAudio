//! Metronome node — generates click sounds synchronized with the transport.
//!
//! Produces a 1kHz sine click (10ms) on regular beats and a 1.5kHz accent
//! click on beat 1 of each bar. No heap allocations in process().

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::graph::node::{AudioNode, ProcessContext};
use ma_core::audio_buffer::AudioBuffer;
use ma_core::ids::NodeId;
use ma_core::parameters::TransportState;

/// Regular click frequency (Hz).
const CLICK_FREQ: f64 = 1000.0;

/// Accent click frequency (Hz) — beat 1.
const ACCENT_FREQ: f64 = 1500.0;

/// Click duration in seconds.
const CLICK_DURATION_SECS: f64 = 0.01;

/// Click amplitude (0.0–1.0).
const CLICK_AMPLITUDE: f32 = 0.5;

/// Metronome audio node — 0 inputs, 1 stereo output.
pub struct MetronomeNode {
    id: NodeId,
    /// Whether the metronome is enabled (shared with UI).
    pub enabled: Arc<AtomicBool>,
    /// Current phase of the click oscillator (0.0–1.0 cycle).
    phase: f64,
    /// Remaining samples in the current click envelope.
    click_samples_remaining: u32,
    /// Frequency of the current click (CLICK_FREQ or ACCENT_FREQ).
    current_freq: f64,
    /// The beat index within the current bar at which the last click was triggered.
    /// Used to detect beat transitions.
    last_beat_index: i64,
    /// Time signature numerator (beats per bar) — cached to avoid repeated reads.
    beats_per_bar: u32,
}

impl MetronomeNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            enabled: Arc::new(AtomicBool::new(false)),
            phase: 0.0,
            click_samples_remaining: 0,
            current_freq: CLICK_FREQ,
            last_beat_index: -1,
            beats_per_bar: 4,
        }
    }

    /// Convert a sample position to a beat index (integer beat number from timeline start).
    #[inline]
    fn sample_to_beat_index(sample: i64, tempo: f64, sample_rate: f32) -> i64 {
        if tempo <= 0.0 || sample_rate <= 0.0 {
            return 0;
        }
        // beat_index = sample * tempo / (60 * sample_rate)
        let beats = sample as f64 * tempo / (60.0 * sample_rate as f64);
        beats.floor() as i64
    }

    /// Convert a sample position to the beat number within the current bar (0-based).
    #[inline]
    fn beat_in_bar(beat_index: i64, beats_per_bar: u32) -> u32 {
        if beats_per_bar == 0 {
            return 0;
        }
        beat_index.rem_euclid(beats_per_bar as i64) as u32
    }
}

impl AudioNode for MetronomeNode {
    fn process(
        &mut self,
        _inputs: &[&AudioBuffer],
        outputs: &mut [&mut AudioBuffer],
        context: &ProcessContext,
    ) {
        let output = match outputs.first_mut() {
            Some(o) => o,
            None => return,
        };

        output.clear();

        // ORDERING: Relaxed OK — single-value eventual consistency
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        // Only produce clicks when transport is playing or recording
        let is_active = matches!(
            context.transport_state,
            TransportState::Playing | TransportState::Recording
        );
        if !is_active {
            self.last_beat_index = -1;
            self.click_samples_remaining = 0;
            self.phase = 0.0;
            return;
        }

        let sample_rate = context.sample_rate;
        let tempo = context.tempo;
        if sample_rate <= 0.0 || tempo <= 0.0 {
            return;
        }
        let frames = context.buffer_size as usize;
        let click_duration_samples = ((CLICK_DURATION_SECS * sample_rate as f64) as u32).max(1);

        for frame in 0..frames {
            let current_sample = context.playhead_samples + frame as i64;
            let beat_index = Self::sample_to_beat_index(current_sample, tempo, sample_rate);

            // Detect new beat
            if beat_index != self.last_beat_index && beat_index >= 0 {
                self.last_beat_index = beat_index;
                self.click_samples_remaining = click_duration_samples;
                self.phase = 0.0;

                // Accent on beat 1 of bar
                let beat_in_bar = Self::beat_in_bar(beat_index, self.beats_per_bar);
                self.current_freq = if beat_in_bar == 0 {
                    ACCENT_FREQ
                } else {
                    CLICK_FREQ
                };
            }

            // Generate click sample
            let sample = if self.click_samples_remaining > 0 {
                self.click_samples_remaining -= 1;

                // Sine oscillator
                let value = (self.phase * std::f64::consts::TAU).sin() as f32 * CLICK_AMPLITUDE;

                // Advance phase
                self.phase += self.current_freq / sample_rate as f64;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }

                // Simple linear envelope (fade out last 20% of click)
                let progress =
                    1.0 - (self.click_samples_remaining as f32 / click_duration_samples as f32);
                let envelope = if progress > 0.8 {
                    (1.0 - progress) / 0.2
                } else {
                    1.0
                };

                value * envelope
            } else {
                0.0
            };

            // Write to both channels (mono click → stereo)
            output.channel_mut(0)[frame] = sample;
            if output.channels() > 1 {
                output.channel_mut(1)[frame] = sample;
            }
        }
    }

    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.click_samples_remaining = 0;
        self.last_beat_index = -1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use ma_core::audio_buffer::AudioBuffer;

    fn make_context(playhead: i64, tempo: f64, sample_rate: f32, frames: u32) -> ProcessContext {
        ProcessContext {
            sample_rate,
            transport_state: TransportState::Playing,
            playhead_samples: playhead,
            tempo,
            buffer_size: frames,
            any_solo: false,
        }
    }

    #[test]
    fn metronome_disabled_produces_silence() {
        let mut node = MetronomeNode::new(NodeId(0));
        // enabled is false by default
        let mut output = AudioBuffer::new(2, 256);
        let context = make_context(0, 120.0, 48000.0, 256);
        node.process(&[], &mut [&mut output], &context);

        for frame in 0..256 {
            assert_eq!(output.channel(0)[frame], 0.0);
            assert_eq!(output.channel(1)[frame], 0.0);
        }
    }

    #[test]
    fn metronome_enabled_produces_click_at_beat() {
        let mut node = MetronomeNode::new(NodeId(0));
        node.enabled.store(true, Ordering::Relaxed);
        node.beats_per_bar = 4;

        let mut output = AudioBuffer::new(2, 256);
        // playhead at exactly beat 0 → should trigger click
        let context = make_context(0, 120.0, 48000.0, 256);
        node.process(&[], &mut [&mut output], &context);

        // First samples should be non-zero (click)
        let has_signal = (0..256).any(|i| output.channel(0)[i] != 0.0);
        assert!(has_signal, "Expected click signal when enabled at beat 0");
    }

    #[test]
    fn metronome_stopped_transport_no_click() {
        let mut node = MetronomeNode::new(NodeId(0));
        node.enabled.store(true, Ordering::Relaxed);

        let mut output = AudioBuffer::new(2, 256);
        let context = ProcessContext {
            sample_rate: 48000.0,
            transport_state: TransportState::Stopped,
            playhead_samples: 0,
            tempo: 120.0,
            buffer_size: 256,
            any_solo: false,
        };
        node.process(&[], &mut [&mut output], &context);

        for frame in 0..256 {
            assert_eq!(output.channel(0)[frame], 0.0);
        }
    }

    #[test]
    fn sample_to_beat_index_basic() {
        // At 120 BPM, 48000 Hz: one beat = 48000 * 60 / 120 = 24000 samples
        assert_eq!(MetronomeNode::sample_to_beat_index(0, 120.0, 48000.0), 0);
        assert_eq!(
            MetronomeNode::sample_to_beat_index(24000, 120.0, 48000.0),
            1
        );
        assert_eq!(
            MetronomeNode::sample_to_beat_index(48000, 120.0, 48000.0),
            2
        );
        assert_eq!(
            MetronomeNode::sample_to_beat_index(23999, 120.0, 48000.0),
            0
        );
    }

    #[test]
    fn beat_in_bar_wraps() {
        assert_eq!(MetronomeNode::beat_in_bar(0, 4), 0);
        assert_eq!(MetronomeNode::beat_in_bar(1, 4), 1);
        assert_eq!(MetronomeNode::beat_in_bar(4, 4), 0);
        assert_eq!(MetronomeNode::beat_in_bar(7, 4), 3);
    }

    #[test]
    fn accent_on_beat_one() {
        let mut node = MetronomeNode::new(NodeId(0));
        node.enabled.store(true, Ordering::Relaxed);
        node.beats_per_bar = 4;

        // Process at beat 0 (accent)
        let mut output = AudioBuffer::new(2, 256);
        let context = make_context(0, 120.0, 48000.0, 256);
        node.process(&[], &mut [&mut output], &context);
        assert_eq!(node.current_freq as u32, ACCENT_FREQ as u32);

        // Process at beat 1 (regular)
        let mut output2 = AudioBuffer::new(2, 256);
        let context2 = make_context(24000, 120.0, 48000.0, 256);
        node.process(&[], &mut [&mut output2], &context2);
        assert_eq!(node.current_freq as u32, CLICK_FREQ as u32);
    }
}
