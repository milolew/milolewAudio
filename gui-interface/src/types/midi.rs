//! MIDI types for note representation, events, and messages.

use serde::{Deserialize, Serialize};

use super::time::Tick;

/// Unique identifier for a MIDI note in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NoteId(pub u64);

/// MIDI message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: i16,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
}

/// A MIDI event with tick-accurate timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiEvent {
    pub tick: Tick,
    pub message: MidiMessage,
}

/// Piano roll note representation (richer than raw MidiEvent).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub pitch: u8,
    pub start_tick: Tick,
    pub duration_ticks: Tick,
    pub velocity: u8,
    pub channel: u8,
}

impl Note {
    pub fn end_tick(&self) -> Tick {
        self.start_tick + self.duration_ticks
    }
}

/// Convert a Note into NoteOn + NoteOff MidiEvents.
pub fn note_to_events(note: &Note) -> [MidiEvent; 2] {
    [
        MidiEvent {
            tick: note.start_tick,
            message: MidiMessage::NoteOn {
                channel: note.channel,
                note: note.pitch,
                velocity: note.velocity,
            },
        },
        MidiEvent {
            tick: note.end_tick(),
            message: MidiMessage::NoteOff {
                channel: note.channel,
                note: note.pitch,
                velocity: 0,
            },
        },
    ]
}

/// Pre-computed MIDI note names (0-127) to avoid per-frame allocations.
const NOTE_NAMES: [&str; 128] = {
    // We can't use format! in const, so we use a macro-generated lookup table.
    // For now, use a runtime-initialized cache via std::sync::LazyLock.
    // Fallback: we'll compute at call site but cache.
    [
        "C-1", "C#-1", "D-1", "D#-1", "E-1", "F-1", "F#-1", "G-1", "G#-1", "A-1", "A#-1", "B-1",
        "C0", "C#0", "D0", "D#0", "E0", "F0", "F#0", "G0", "G#0", "A0", "A#0", "B0",
        "C1", "C#1", "D1", "D#1", "E1", "F1", "F#1", "G1", "G#1", "A1", "A#1", "B1",
        "C2", "C#2", "D2", "D#2", "E2", "F2", "F#2", "G2", "G#2", "A2", "A#2", "B2",
        "C3", "C#3", "D3", "D#3", "E3", "F3", "F#3", "G3", "G#3", "A3", "A#3", "B3",
        "C4", "C#4", "D4", "D#4", "E4", "F4", "F#4", "G4", "G#4", "A4", "A#4", "B4",
        "C5", "C#5", "D5", "D#5", "E5", "F5", "F#5", "G5", "G#5", "A5", "A#5", "B5",
        "C6", "C#6", "D6", "D#6", "E6", "F6", "F#6", "G6", "G#6", "A6", "A#6", "B6",
        "C7", "C#7", "D7", "D#7", "E7", "F7", "F#7", "G7", "G#7", "A7", "A#7", "B7",
        "C8", "C#8", "D8", "D#8", "E8", "F8", "F#8", "G8", "G#8", "A8", "A#8", "B8",
        "C9", "C#9", "D9", "D#9", "E9", "F9", "F#9", "G9",
    ]
};

/// MIDI note number to name (e.g., 60 -> "C4"). Zero allocations.
pub fn note_name(pitch: u8) -> &'static str {
    NOTE_NAMES[pitch as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_end_tick() {
        let note = Note {
            id: NoteId(1),
            pitch: 60,
            start_tick: 0,
            duration_ticks: 960,
            velocity: 100,
            channel: 0,
        };
        assert_eq!(note.end_tick(), 960);
    }

    #[test]
    fn test_note_name() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(0), "C-1");
    }

    #[test]
    fn test_note_to_events() {
        let note = Note {
            id: NoteId(1),
            pitch: 64,
            start_tick: 480,
            duration_ticks: 240,
            velocity: 90,
            channel: 0,
        };
        let events = note_to_events(&note);
        assert_eq!(events[0].tick, 480);
        assert_eq!(events[1].tick, 720);
    }
}
