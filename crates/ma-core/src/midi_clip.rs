//! MIDI clip — a sorted sequence of MIDI events for timeline playback.
//!
//! A `MidiClip` stores events with tick-based timing relative to the clip start
//! (tick 0 = first beat of the clip). Clips are immutable after construction and
//! designed to be shared across threads via `Arc<MidiClip>`.
//!
//! `MidiClipRef` places a clip on the timeline at an absolute tick position,
//! analogous to `AudioClipRef` in the audio engine.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ids::ClipId;
use crate::parameters::MidiEvent;
use crate::time::Tick;

/// A MIDI clip containing a sorted sequence of timed MIDI events.
///
/// Events are stored sorted by tick for efficient range queries via binary search.
/// All ticks are relative to the clip start (tick 0 = clip start).
///
/// # Thread Safety
/// `MidiClip` is `Send + Sync` and intended to be shared via `Arc<MidiClip>`.
/// The audio thread reads events but never drops the last `Arc` reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiClip {
    /// Events sorted by tick (ascending). Relative to clip start.
    events: Vec<MidiEvent>,
    /// Total duration of the clip in ticks. May extend beyond the last event.
    duration_ticks: Tick,
}

impl MidiClip {
    /// Create a new MIDI clip. Events are sorted by tick automatically.
    ///
    /// `duration_ticks` defines the clip length on the timeline and may be
    /// longer than the last event's tick (e.g., a clip with trailing silence).
    pub fn new(mut events: Vec<MidiEvent>, duration_ticks: Tick) -> Self {
        events.sort_by_key(|e| e.tick);
        Self {
            events,
            duration_ticks: duration_ticks.max(0),
        }
    }

    /// All events in the clip, sorted by tick.
    #[inline]
    pub fn events(&self) -> &[MidiEvent] {
        &self.events
    }

    /// Events within the tick range `[start_tick, end_tick)`.
    ///
    /// Uses binary search for O(log n) lookup — safe for real-time use (no allocations).
    pub fn events_in_range(&self, start_tick: Tick, end_tick: Tick) -> &[MidiEvent] {
        if start_tick >= end_tick || self.events.is_empty() {
            return &[];
        }

        let lo = self.events.partition_point(|e| e.tick < start_tick);
        let hi = self.events.partition_point(|e| e.tick < end_tick);

        &self.events[lo..hi]
    }

    /// Total duration of the clip in ticks.
    #[inline]
    pub fn duration_ticks(&self) -> Tick {
        self.duration_ticks
    }

    /// Whether the clip contains zero events.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Number of events in the clip.
    #[inline]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Merge another clip's events into this one, returning a new combined clip.
    ///
    /// `other_offset` is the tick offset of `other` relative to this clip's start.
    /// The resulting clip's duration is the maximum of both clips' extents.
    /// Events are sorted by tick in the output.
    pub fn merged_with(&self, other: &MidiClip, other_offset: Tick) -> MidiClip {
        let mut combined = self.events.clone();
        for event in &other.events {
            combined.push(MidiEvent {
                tick: event.tick + other_offset,
                message: event.message,
            });
        }
        let max_duration = self.duration_ticks.max(other.duration_ticks + other_offset);
        MidiClip::new(combined, max_duration)
    }
}

/// A reference to a MIDI clip placed at a specific position on the timeline.
///
/// Analogous to `AudioClipRef` in the audio engine's WAV player node.
/// Uses `Arc<MidiClip>` so the audio thread can read events without owning them.
pub struct MidiClipRef {
    /// Unique identifier for this clip instance.
    pub clip_id: ClipId,
    /// Shared reference to the clip data. The project/UI holds another Arc,
    /// so the audio thread never drops the last reference.
    pub clip: Arc<MidiClip>,
    /// Absolute position on the timeline where this clip starts (in ticks).
    pub start_tick: Tick,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameters::MidiMessage;

    fn note_on_event(tick: Tick, note: u8, velocity: u8) -> MidiEvent {
        MidiEvent {
            tick,
            message: MidiMessage::NoteOn {
                channel: 0,
                note,
                velocity,
            },
        }
    }

    fn note_off_event(tick: Tick, note: u8) -> MidiEvent {
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
    fn new_sorts_events_by_tick() {
        let events = vec![
            note_on_event(960, 60, 100),
            note_on_event(0, 64, 80),
            note_on_event(480, 67, 90),
        ];
        let clip = MidiClip::new(events, 1920);

        assert_eq!(clip.events()[0].tick, 0);
        assert_eq!(clip.events()[1].tick, 480);
        assert_eq!(clip.events()[2].tick, 960);
    }

    #[test]
    fn new_clamps_negative_duration() {
        let clip = MidiClip::new(vec![], -100);
        assert_eq!(clip.duration_ticks(), 0);
    }

    #[test]
    fn events_in_range_full_overlap() {
        let events = vec![
            note_on_event(0, 60, 100),
            note_on_event(480, 64, 80),
            note_on_event(960, 67, 90),
        ];
        let clip = MidiClip::new(events, 1920);

        let range = clip.events_in_range(0, 1920);
        assert_eq!(range.len(), 3);
    }

    #[test]
    fn events_in_range_partial_overlap() {
        let events = vec![
            note_on_event(0, 60, 100),
            note_on_event(480, 64, 80),
            note_on_event(960, 67, 90),
            note_off_event(1440, 60),
        ];
        let clip = MidiClip::new(events, 1920);

        let range = clip.events_in_range(480, 960);
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].tick, 480);
    }

    #[test]
    fn events_in_range_no_overlap() {
        let events = vec![note_on_event(0, 60, 100), note_on_event(480, 64, 80)];
        let clip = MidiClip::new(events, 960);

        let range = clip.events_in_range(960, 1920);
        assert_eq!(range.len(), 0);
    }

    #[test]
    fn events_in_range_empty_clip() {
        let clip = MidiClip::new(vec![], 960);
        let range = clip.events_in_range(0, 960);
        assert_eq!(range.len(), 0);
    }

    #[test]
    fn events_in_range_inverted_range_returns_empty() {
        let events = vec![note_on_event(480, 60, 100)];
        let clip = MidiClip::new(events, 960);

        let range = clip.events_in_range(960, 0);
        assert_eq!(range.len(), 0);
    }

    #[test]
    fn events_in_range_exact_boundaries() {
        let events = vec![
            note_on_event(100, 60, 100),
            note_on_event(200, 64, 80),
            note_on_event(300, 67, 90),
        ];
        let clip = MidiClip::new(events, 960);

        // start_tick inclusive, end_tick exclusive
        let range = clip.events_in_range(200, 300);
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].tick, 200);
    }

    #[test]
    fn is_empty_and_event_count() {
        let empty = MidiClip::new(vec![], 960);
        assert!(empty.is_empty());
        assert_eq!(empty.event_count(), 0);

        let filled = MidiClip::new(vec![note_on_event(0, 60, 100)], 960);
        assert!(!filled.is_empty());
        assert_eq!(filled.event_count(), 1);
    }

    #[test]
    fn duration_independent_of_events() {
        let clip = MidiClip::new(vec![note_on_event(0, 60, 100)], 3840);
        assert_eq!(clip.duration_ticks(), 3840);
        assert_eq!(clip.event_count(), 1);
    }

    #[test]
    fn serialization_round_trip() {
        let events = vec![note_on_event(0, 60, 100), note_off_event(480, 60)];
        let clip = MidiClip::new(events, 960);

        let json = serde_json::to_string(&clip).unwrap();
        let deserialized: MidiClip = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.event_count(), 2);
        assert_eq!(deserialized.duration_ticks(), 960);
        assert_eq!(deserialized.events()[0].tick, 0);
        assert_eq!(deserialized.events()[1].tick, 480);
    }

    #[test]
    fn midi_clip_ref_creation() {
        let clip = Arc::new(MidiClip::new(vec![note_on_event(0, 60, 100)], 960));
        let clip_ref = MidiClipRef {
            clip_id: ClipId::new(),
            clip: Arc::clone(&clip),
            start_tick: 3840,
        };

        assert_eq!(clip_ref.start_tick, 3840);
        assert_eq!(clip_ref.clip.event_count(), 1);
        // Two Arc references: clip + clip_ref.clip
        assert_eq!(Arc::strong_count(&clip), 2);
    }

    #[test]
    fn duplicate_tick_events_preserved() {
        let events = vec![
            note_on_event(480, 60, 100),
            note_on_event(480, 64, 80),
            note_on_event(480, 67, 90),
        ];
        let clip = MidiClip::new(events, 960);

        let range = clip.events_in_range(480, 481);
        assert_eq!(range.len(), 3);
    }

    #[test]
    fn events_in_range_single_event_at_boundary() {
        let events = vec![note_on_event(0, 60, 100)];
        let clip = MidiClip::new(events, 960);

        // Event at tick 0 should be in [0, 1)
        assert_eq!(clip.events_in_range(0, 1).len(), 1);
        // Event at tick 0 should NOT be in [1, 2)
        assert_eq!(clip.events_in_range(1, 2).len(), 0);
    }

    #[test]
    fn merged_with_combines_and_sorts() {
        let clip_a = MidiClip::new(
            vec![note_on_event(0, 60, 100), note_on_event(480, 64, 80)],
            960,
        );
        let clip_b = MidiClip::new(
            vec![note_on_event(0, 67, 90), note_on_event(240, 72, 70)],
            480,
        );

        let merged = clip_a.merged_with(&clip_b, 0);
        assert_eq!(merged.event_count(), 4);
        assert_eq!(merged.duration_ticks(), 960);
        // Events should be sorted by tick
        assert_eq!(merged.events()[0].tick, 0);
        assert_eq!(merged.events()[1].tick, 0);
        assert_eq!(merged.events()[2].tick, 240);
        assert_eq!(merged.events()[3].tick, 480);
    }

    #[test]
    fn merged_with_offset_shifts_other_events() {
        let clip_a = MidiClip::new(vec![note_on_event(0, 60, 100)], 480);
        let clip_b = MidiClip::new(vec![note_on_event(0, 64, 80)], 480);

        let merged = clip_a.merged_with(&clip_b, 480);
        assert_eq!(merged.event_count(), 2);
        assert_eq!(merged.events()[0].tick, 0);
        assert_eq!(merged.events()[1].tick, 480);
        assert_eq!(merged.duration_ticks(), 960); // max(480, 480+480)
    }

    #[test]
    fn merged_with_empty_clip() {
        let clip_a = MidiClip::new(vec![note_on_event(0, 60, 100)], 960);
        let clip_b = MidiClip::new(vec![], 0);

        let merged = clip_a.merged_with(&clip_b, 0);
        assert_eq!(merged.event_count(), 1);
        assert_eq!(merged.duration_ticks(), 960);
    }
}
