//! UI event definitions — all events the views/widgets can emit.

use vizia::prelude::*;

use crate::state::piano_roll_state::PianoRollInteraction;
use crate::types::midi::{Note, NoteId};
use crate::types::time::{QuantizeGrid, Tick};
use crate::types::track::{ClipId, TrackId};

/// Which main view is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Data)]
pub enum ActiveView {
    Arrangement,
    Mixer,
    PianoRoll,
}

/// All events the UI can emit — consolidated from all view/widget actions.
pub enum AppEvent {
    /// Timer-driven poll of the engine ring buffer.
    PollEngine,
    /// Initialize the polling timer (called once on startup).
    InitTimer,

    // -- Preferences --
    ShowPreferences,
    HidePreferences,
    RefreshDevices,

    // -- Transport --
    Play,
    Stop,
    Record,
    Pause,
    SetTempo(f64),
    SetPosition(Tick),
    ToggleLoop,

    // -- View switching --
    SwitchView(ActiveView),
    OpenPianoRoll(ClipId),

    // -- Track selection --
    SelectTrack(TrackId),

    // -- Mixer --
    SetTrackVolume {
        track_id: TrackId,
        volume: f32,
    },
    SetTrackPan {
        track_id: TrackId,
        pan: f32,
    },
    ToggleMute(TrackId),
    ToggleSolo(TrackId),

    // -- Arrangement scroll/zoom --
    ScrollArrangementX(f64),
    ScrollArrangementY(f32),
    ZoomArrangement(f64),

    // -- Piano roll --
    AddNote(Note),
    RemoveNote(NoteId),
    MoveNote {
        note_id: NoteId,
        new_start: Tick,
        new_pitch: u8,
    },
    ResizeNote {
        note_id: NoteId,
        new_duration: Tick,
    },
    PreviewNoteOn {
        note: u8,
        velocity: u8,
    },
    PreviewNoteOff {
        note: u8,
    },
    UpdateInteraction(PianoRollInteraction),
    SetQuantize(QuantizeGrid),

    // -- Piano roll scroll/zoom --
    ScrollPianoRollX(f64),
    ScrollPianoRollY(i8),
    ZoomPianoRoll(f64),
}
