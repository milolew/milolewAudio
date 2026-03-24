//! Commands sent from the UI to the audio engine via SPSC ring buffer.

use crate::types::midi::{Note, NoteId};
use crate::types::time::{Tick, TimeSignature};
use crate::types::track::{ClipId, TrackId};

/// All commands the UI can send to the audio engine.
#[derive(Debug, Clone)]
pub enum EngineCommand {
    // Transport
    Play,
    Stop,
    Record,
    Pause,
    SetPosition(Tick),
    SetTempo(f64),
    SetTimeSignature(TimeSignature),

    // Track parameters
    SetTrackVolume {
        track_id: TrackId,
        volume: f32,
    },
    SetTrackPan {
        track_id: TrackId,
        pan: f32,
    },
    SetTrackMute {
        track_id: TrackId,
        mute: bool,
    },
    SetTrackSolo {
        track_id: TrackId,
        solo: bool,
    },

    // MIDI preview (live playing from UI)
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

    // Clip note editing
    AddNote {
        clip_id: ClipId,
        note: Note,
    },
    RemoveNote {
        clip_id: ClipId,
        note_id: NoteId,
    },
    MoveNote {
        clip_id: ClipId,
        note_id: NoteId,
        new_start: Tick,
        new_pitch: u8,
    },
    ResizeNote {
        clip_id: ClipId,
        note_id: NoteId,
        new_duration: Tick,
    },
}
