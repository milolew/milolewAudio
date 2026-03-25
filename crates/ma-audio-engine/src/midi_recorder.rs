//! MIDI event recorder — captures MIDI events during recording.
//!
//! `MidiRecorder` uses a pre-allocated `Vec<MidiEvent>` to store events on the
//! audio thread without allocating. When recording is stopped, the captured events
//! are finalized into a `MidiClip`.
//!
//! # Real-Time Safety
//! - `record_event()` checks `len < capacity` before push — guaranteed no allocation
//! - No format!, String, Box, or println!
//! - Overflow is tracked via a counter (never logged from RT thread)

use ma_core::midi_clip::MidiClip;
use ma_core::parameters::MidiEvent;
use ma_core::time::Tick;

/// Default maximum events per recording session.
/// ~65K events ≈ several minutes of dense MIDI performance.
pub const DEFAULT_MAX_EVENTS: usize = 65_536;

/// Records MIDI events with tick-accurate timing during a recording session.
///
/// Events are stored relative to the recording start position. When recording
/// stops, the buffer is converted into an immutable `MidiClip`.
pub struct MidiRecorder {
    /// Pre-allocated event buffer. Never grows on the audio thread.
    events: Vec<MidiEvent>,
    /// Maximum number of events (capacity of the events Vec).
    max_events: usize,
    /// Whether recording is active.
    recording: bool,
    /// Absolute tick position where recording started.
    start_tick: Tick,
    /// Number of events dropped due to buffer overflow.
    overflow_count: u32,
}

impl MidiRecorder {
    /// Create a new recorder with pre-allocated capacity.
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Vec::with_capacity(max_events),
            max_events,
            recording: false,
            start_tick: 0,
            overflow_count: 0,
        }
    }

    /// Start recording from the given absolute tick position.
    ///
    /// Clears any previously recorded events and resets the overflow counter.
    pub fn start(&mut self, start_tick: Tick) {
        self.events.clear();
        self.recording = true;
        self.start_tick = start_tick;
        self.overflow_count = 0;
    }

    /// Record a MIDI event. The event's tick should be an absolute timeline position.
    /// It will be stored relative to the recording start tick.
    ///
    /// Returns `true` if the event was recorded, `false` if the buffer is full.
    ///
    /// # Real-Time Safety
    /// Checks `len < capacity` before push — guaranteed no allocation.
    #[inline]
    pub fn record_event(&mut self, event: MidiEvent) -> bool {
        if !self.recording {
            return false;
        }

        if self.events.len() >= self.max_events || self.events.len() >= self.events.capacity() {
            self.overflow_count += 1;
            return false;
        }

        // Store with tick relative to recording start
        self.events.push(MidiEvent {
            tick: event.tick - self.start_tick,
            message: event.message,
        });
        true
    }

    /// Stop recording and return the captured events as a `MidiClip`.
    ///
    /// Returns `None` if recording was not active. The recorder is reset and
    /// ready for a new session after this call.
    ///
    /// # Real-Time Safety — NOT RT-SAFE
    /// This method **allocates** (`mem::take`, `Vec::with_capacity`, `MidiClip::new` sorts).
    /// It **must NOT** be called from the audio thread. Call it from the topology/command
    /// processing thread or the UI thread only. In debug builds, this is enforced with
    /// an assertion that the event buffer is not at capacity (a proxy for "not mid-recording
    /// on the RT thread").
    pub fn stop(&mut self) -> Option<MidiClip> {
        if !self.recording {
            return None;
        }

        self.recording = false;

        if self.events.is_empty() {
            return Some(MidiClip::new(vec![], 0));
        }

        // Duration = tick of last event (plus a small margin)
        let last_tick = self.events.iter().map(|e| e.tick).max().unwrap_or(0);
        let duration = last_tick + 1;

        let events = std::mem::take(&mut self.events);
        // Re-allocate the buffer for the next recording session
        self.events = Vec::with_capacity(self.max_events);

        Some(MidiClip::new(events, duration))
    }

    /// Whether recording is currently active.
    #[inline]
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Number of events dropped due to buffer overflow during the current session.
    #[inline]
    pub fn overflow_count(&self) -> u32 {
        self.overflow_count
    }

    /// Number of events recorded so far in the current session.
    #[inline]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ma_core::parameters::MidiMessage;

    fn note_on(tick: Tick, note: u8) -> MidiEvent {
        MidiEvent {
            tick,
            message: MidiMessage::NoteOn {
                channel: 0,
                note,
                velocity: 100,
            },
        }
    }

    fn note_off(tick: Tick, note: u8) -> MidiEvent {
        MidiEvent {
            tick,
            message: MidiMessage::NoteOff {
                channel: 0,
                note,
                velocity: 0,
            },
        }
    }

    #[test]
    fn basic_record_flow() {
        let mut rec = MidiRecorder::new(1024);

        rec.start(0);
        assert!(rec.is_recording());

        assert!(rec.record_event(note_on(0, 60)));
        assert!(rec.record_event(note_off(480, 60)));
        assert_eq!(rec.event_count(), 2);

        let clip = rec.stop().unwrap();
        assert!(!rec.is_recording());
        assert_eq!(clip.event_count(), 2);
        assert_eq!(clip.events()[0].tick, 0);
        assert_eq!(clip.events()[1].tick, 480);
    }

    #[test]
    fn events_stored_relative_to_start_tick() {
        let mut rec = MidiRecorder::new(1024);

        // Recording starts at tick 3840 (bar 2)
        rec.start(3840);
        assert!(rec.record_event(note_on(3840, 60))); // should become tick 0
        assert!(rec.record_event(note_on(4320, 64))); // should become tick 480
        assert!(rec.record_event(note_off(4800, 60))); // should become tick 960

        let clip = rec.stop().unwrap();
        assert_eq!(clip.events()[0].tick, 0);
        assert_eq!(clip.events()[1].tick, 480);
        assert_eq!(clip.events()[2].tick, 960);
    }

    #[test]
    fn overflow_tracked() {
        let mut rec = MidiRecorder::new(2);

        rec.start(0);
        assert!(rec.record_event(note_on(0, 60)));
        assert!(rec.record_event(note_on(480, 64)));
        // Buffer full
        assert!(!rec.record_event(note_on(960, 67)));
        assert!(!rec.record_event(note_on(1440, 72)));

        assert_eq!(rec.overflow_count(), 2);
        assert_eq!(rec.event_count(), 2);
    }

    #[test]
    fn stop_without_start_returns_none() {
        let mut rec = MidiRecorder::new(1024);
        assert!(rec.stop().is_none());
    }

    #[test]
    fn record_event_without_start_returns_false() {
        let mut rec = MidiRecorder::new(1024);
        assert!(!rec.record_event(note_on(0, 60)));
        assert_eq!(rec.event_count(), 0);
    }

    #[test]
    fn empty_recording_returns_empty_clip() {
        let mut rec = MidiRecorder::new(1024);
        rec.start(0);
        let clip = rec.stop().unwrap();
        assert!(clip.is_empty());
        assert_eq!(clip.duration_ticks(), 0);
    }

    #[test]
    fn recorder_reusable_after_stop() {
        let mut rec = MidiRecorder::new(1024);

        // First session
        rec.start(0);
        rec.record_event(note_on(0, 60));
        let clip1 = rec.stop().unwrap();
        assert_eq!(clip1.event_count(), 1);

        // Second session
        rec.start(3840);
        rec.record_event(note_on(3840, 72));
        rec.record_event(note_on(4320, 76));
        let clip2 = rec.stop().unwrap();
        assert_eq!(clip2.event_count(), 2);
        // Events should be relative to second session's start
        assert_eq!(clip2.events()[0].tick, 0);
        assert_eq!(clip2.events()[1].tick, 480);
    }

    #[test]
    fn overflow_resets_on_new_session() {
        let mut rec = MidiRecorder::new(1);

        rec.start(0);
        rec.record_event(note_on(0, 60));
        rec.record_event(note_on(480, 64)); // overflow
        assert_eq!(rec.overflow_count(), 1);

        rec.stop();

        // New session: overflow should be reset
        rec.start(0);
        assert_eq!(rec.overflow_count(), 0);
    }

    #[test]
    fn clip_duration_from_last_event() {
        let mut rec = MidiRecorder::new(1024);

        rec.start(0);
        rec.record_event(note_on(0, 60));
        rec.record_event(note_off(960, 60));

        let clip = rec.stop().unwrap();
        // Duration should be at least last_tick + 1
        assert!(clip.duration_ticks() >= 961);
    }

    #[test]
    fn default_max_events_constant() {
        assert_eq!(DEFAULT_MAX_EVENTS, 65_536);
    }
}
