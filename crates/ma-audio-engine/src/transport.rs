//! Transport system — play, stop, pause, seek, loop.
//!
//! The transport manages the playhead position and state machine.
//! It runs entirely on the audio thread and updates atomically.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use ma_core::parameters::TransportState;
use ma_core::time::{FrameCount, SamplePos};

/// The transport controls playback position and state.
///
/// State is stored in atomics for lock-free read access from the UI thread.
/// Mutations happen only on the audio thread (via command processing).
pub struct Transport {
    /// Current playhead position in samples.
    position: Arc<AtomicI64>,

    /// Position where playback started (for Stop → return to start).
    play_start_position: SamplePos,

    /// Current state.
    state: TransportState,

    /// Tempo in BPM.
    tempo: f64,

    /// Sample rate.
    sample_rate: f64,

    /// Loop region.
    loop_start: SamplePos,
    loop_end: SamplePos,
    loop_enabled: bool,

    /// Whether recording is active.
    is_recording: Arc<AtomicBool>,

    // ── Count-in state ──
    /// Number of bars for count-in (0 = no count-in).
    count_in_bars: u8,
    /// Current bar within count-in (0-based).
    count_in_bar: u8,
    /// Current beat within bar (0-based).
    count_in_beat: u8,
    /// Samples per beat (computed from tempo and sample_rate).
    count_in_samples_per_beat: i64,
    /// Sample counter within current beat.
    count_in_sample_counter: i64,
}

impl Transport {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            position: Arc::new(AtomicI64::new(0)),
            play_start_position: 0,
            state: TransportState::Stopped,
            tempo: 120.0,
            sample_rate,
            loop_start: 0,
            loop_end: 0,
            loop_enabled: false,
            is_recording: Arc::new(AtomicBool::new(false)),
            count_in_bars: 0,
            count_in_bar: 0,
            count_in_beat: 0,
            count_in_samples_per_beat: 0,
            count_in_sample_counter: 0,
        }
    }

    /// Get an Arc to the position atomic (for UI to read).
    pub fn position_atomic(&self) -> Arc<AtomicI64> {
        Arc::clone(&self.position)
    }

    /// Get an Arc to the recording state atomic.
    pub fn is_recording_atomic(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_recording)
    }

    /// Current playhead position in samples.
    #[inline]
    pub fn position(&self) -> SamplePos {
        // ORDERING: Relaxed OK — audio thread reading its own atomic (writer == reader)
        self.position.load(Ordering::Relaxed)
    }

    /// Current transport state.
    #[inline]
    pub fn state(&self) -> TransportState {
        self.state
    }

    /// Current tempo in BPM.
    #[inline]
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Sample rate.
    #[inline]
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Start playback.
    pub fn play(&mut self) {
        if self.state == TransportState::Stopped {
            self.play_start_position = self.position();
        }
        self.state = TransportState::Playing;
    }

    /// Stop playback and return to the position where play was started.
    pub fn stop(&mut self) {
        self.state = TransportState::Stopped;
        // ORDERING: Release — cross-thread state read by UI with Acquire
        self.is_recording.store(false, Ordering::Release);
        self.position
            .store(self.play_start_position, Ordering::Release);
    }

    /// Pause at current position.
    pub fn pause(&mut self) {
        if matches!(
            self.state,
            TransportState::Playing | TransportState::Recording | TransportState::CountingIn
        ) {
            self.state = TransportState::Paused;
        }
    }

    /// Seek to a specific sample position.
    pub fn set_position(&mut self, pos: SamplePos) {
        // ORDERING: Release — cross-thread state read by UI with Acquire
        self.position.store(pos, Ordering::Release);
        if self.state == TransportState::Stopped {
            self.play_start_position = pos;
        }
    }

    /// Set tempo in BPM.
    pub fn set_tempo(&mut self, bpm: f64) {
        self.tempo = bpm.clamp(20.0, 999.0);
    }

    /// Configure loop region.
    pub fn set_loop(&mut self, start: SamplePos, end: SamplePos, enabled: bool) {
        self.loop_start = start;
        self.loop_end = end;
        self.loop_enabled = enabled;
    }

    /// Start recording (transport must be playing).
    pub fn start_recording(&mut self) {
        if self.state == TransportState::Stopped {
            self.play();
        }
        self.state = TransportState::Recording;
        // ORDERING: Release — cross-thread state read by UI with Acquire
        self.is_recording.store(true, Ordering::Release);
    }

    /// Stop recording (keeps playing).
    pub fn stop_recording(&mut self) {
        // ORDERING: Release — cross-thread state read by UI with Acquire
        self.is_recording.store(false, Ordering::Release);
        if self.state == TransportState::Recording {
            self.state = TransportState::Playing;
        }
    }

    /// Start a count-in pre-roll before recording.
    ///
    /// During count-in, the metronome plays but the playhead does not advance
    /// and recording does not start until count-in completes.
    pub fn start_count_in(&mut self, bars: u8) {
        if self.state == TransportState::Stopped {
            self.play_start_position = self.position();
        }
        self.count_in_bars = bars;
        self.count_in_bar = 0;
        self.count_in_beat = 0;
        let beats_per_second = self.tempo / 60.0;
        self.count_in_samples_per_beat = (self.sample_rate / beats_per_second) as i64;
        self.count_in_sample_counter = 0;
        self.state = TransportState::CountingIn;
    }

    /// Advance the count-in counter by `frames` samples.
    ///
    /// Returns `Some((bar, beat, is_complete))` whenever a beat boundary is crossed.
    /// `is_complete` is true when the final beat of the count-in is reached,
    /// at which point the caller should transition to Recording.
    ///
    /// The playhead does NOT advance during count-in.
    pub fn advance_count_in(&mut self, frames: FrameCount) -> Option<(u8, u8, bool)> {
        if self.state != TransportState::CountingIn {
            return None;
        }

        self.count_in_sample_counter += frames as i64;

        if self.count_in_sample_counter >= self.count_in_samples_per_beat {
            self.count_in_sample_counter -= self.count_in_samples_per_beat;

            let bar = self.count_in_bar;
            let beat = self.count_in_beat;

            // Advance to next beat
            self.count_in_beat += 1;
            if self.count_in_beat >= 4 {
                // 4/4 time signature (hardcoded for now)
                self.count_in_beat = 0;
                self.count_in_bar += 1;
            }

            let is_complete = self.count_in_bar >= self.count_in_bars;
            Some((bar, beat, is_complete))
        } else {
            None
        }
    }

    /// Total count-in bars.
    pub fn count_in_bars(&self) -> u8 {
        self.count_in_bars
    }

    /// Advance the playhead by one buffer's worth of frames.
    /// Called at the beginning of each audio callback.
    ///
    /// Returns the playhead position at the START of this buffer
    /// (the position used for rendering this buffer).
    #[inline]
    pub fn advance(&mut self, frames: FrameCount) -> SamplePos {
        let current = self.position();

        match self.state {
            TransportState::Playing | TransportState::Recording => {
                let mut new_pos = current + frames as SamplePos;

                // Handle loop
                if self.loop_enabled && self.loop_end > self.loop_start && new_pos >= self.loop_end
                {
                    new_pos = self.loop_start + (new_pos - self.loop_end);
                }

                // ORDERING: Release — cross-thread state read by UI with Acquire
                self.position.store(new_pos, Ordering::Release);
            }
            TransportState::Stopped | TransportState::Paused | TransportState::CountingIn => {
                // Don't advance (count-in uses its own counter)
            }
        }

        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_starts_stopped() {
        let t = Transport::new(48000.0);
        assert_eq!(t.state(), TransportState::Stopped);
        assert_eq!(t.position(), 0);
    }

    #[test]
    fn play_stop_returns_to_start() {
        let mut t = Transport::new(48000.0);
        t.set_position(1000);
        t.play();
        t.advance(256);
        assert_eq!(t.position(), 1256);
        t.stop();
        assert_eq!(t.position(), 1000);
    }

    #[test]
    fn pause_keeps_position() {
        let mut t = Transport::new(48000.0);
        t.play();
        t.advance(256);
        t.pause();
        let pos = t.position();
        t.advance(256); // Should not advance while paused
        assert_eq!(t.position(), pos);
    }

    #[test]
    fn loop_wraps_around() {
        let mut t = Transport::new(48000.0);
        t.set_loop(0, 1000, true);
        t.set_position(900);
        t.play();
        t.advance(256); // 900 + 256 = 1156, loops to 0 + 156 = 156
        assert_eq!(t.position(), 156);
    }

    #[test]
    fn recording_sets_flag() {
        let mut t = Transport::new(48000.0);
        assert!(!t.is_recording.load(Ordering::Relaxed));
        t.start_recording();
        assert!(t.is_recording.load(Ordering::Relaxed));
        assert_eq!(t.state(), TransportState::Recording);
        t.stop_recording();
        assert!(!t.is_recording.load(Ordering::Relaxed));
        assert_eq!(t.state(), TransportState::Playing);
    }

    #[test]
    fn tempo_clamped() {
        let mut t = Transport::new(48000.0);
        t.set_tempo(5.0);
        assert_eq!(t.tempo(), 20.0);
        t.set_tempo(5000.0);
        assert_eq!(t.tempo(), 999.0);
    }
}
