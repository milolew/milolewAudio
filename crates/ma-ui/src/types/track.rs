//! Track and clip types for project structure.

use serde::{Deserialize, Serialize};

use super::midi::Note;
use super::time::Tick;

// Re-export ID types from ma-core for unified type system.
pub use ma_core::ids::{ClipId, TrackId};

/// Track type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Midi,
}

/// Track metadata and state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackState {
    pub id: TrackId,
    pub name: String,
    pub kind: TrackKind,
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub color: [u8; 3],
}

impl TrackState {
    pub fn new_midi(id: TrackId, name: impl Into<String>, color: [u8; 3]) -> Self {
        Self {
            id,
            name: name.into(),
            kind: TrackKind::Midi,
            volume: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            color,
        }
    }

    pub fn new_audio(id: TrackId, name: impl Into<String>, color: [u8; 3]) -> Self {
        Self {
            id,
            name: name.into(),
            kind: TrackKind::Audio,
            volume: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            color,
        }
    }
}

/// Clip data — a region on a track containing notes or audio reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipState {
    pub id: ClipId,
    pub track_id: TrackId,
    pub start_tick: Tick,
    pub duration_ticks: Tick,
    pub name: String,
    pub notes: Vec<Note>,
}

impl ClipState {
    pub fn end_tick(&self) -> Tick {
        self.start_tick + self.duration_ticks
    }

    /// Return a new ClipState with a note added (immutable pattern).
    pub fn with_note_added(&self, note: Note) -> Self {
        let mut new_notes = self.notes.clone();
        new_notes.push(note);
        new_notes.sort_by_key(|n| n.start_tick);
        Self {
            id: self.id,
            track_id: self.track_id,
            start_tick: self.start_tick,
            duration_ticks: self.duration_ticks,
            name: self.name.clone(),
            notes: new_notes,
        }
    }

    /// Return a new ClipState with a note removed (immutable pattern).
    pub fn with_note_removed(&self, note_id: super::midi::NoteId) -> Self {
        Self {
            id: self.id,
            track_id: self.track_id,
            start_tick: self.start_tick,
            duration_ticks: self.duration_ticks,
            name: self.name.clone(),
            notes: self
                .notes
                .iter()
                .filter(|n| n.id != note_id)
                .copied()
                .collect(),
        }
    }

    /// Return a new ClipState with a note replaced (immutable pattern).
    pub fn with_note_updated(&self, updated: Note) -> Self {
        Self {
            id: self.id,
            track_id: self.track_id,
            start_tick: self.start_tick,
            duration_ticks: self.duration_ticks,
            name: self.name.clone(),
            notes: self
                .notes
                .iter()
                .map(|n| if n.id == updated.id { updated } else { *n })
                .collect(),
        }
    }
}
