//! Concrete undo action types for all undoable operations in the DAW.
//!
//! Each action implements [`UndoAction<AppData>`] and captures enough state
//! to perform both forward (apply/redo) and backward (revert/undo) operations.

use ma_core::undo::UndoAction;

use crate::app_data::AppData;
use crate::engine_bridge::commands::EngineCommand;
use crate::types::midi::{Note, NoteId};
use crate::types::time::{QuantizeGrid, Tick};
use crate::types::track::{ClipId, ClipState, TrackId, TrackState};

// ---------------------------------------------------------------------------
// 1. MoveClipAction
// ---------------------------------------------------------------------------

pub struct MoveClipAction {
    pub clip_id: ClipId,
    pub old_start_tick: Tick,
    pub new_start_tick: Tick,
}

impl UndoAction<AppData> for MoveClipAction {
    fn description(&self) -> &str {
        "Move Clip"
    }

    fn apply(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter_mut().find(|c| c.id == self.clip_id) {
            clip.start_tick = self.new_start_tick;
        }
    }

    fn revert(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter_mut().find(|c| c.id == self.clip_id) {
            clip.start_tick = self.old_start_tick;
        }
    }
}

// ---------------------------------------------------------------------------
// 2. AddClipAction
// ---------------------------------------------------------------------------

pub struct AddClipAction {
    pub clip: ClipState,
}

impl UndoAction<AppData> for AddClipAction {
    fn description(&self) -> &str {
        "Add Clip"
    }

    fn apply(&self, state: &mut AppData) {
        state.clips.push(self.clip.clone());
    }

    fn revert(&self, state: &mut AppData) {
        state.clips.retain(|c| c.id != self.clip.id);
    }
}

// ---------------------------------------------------------------------------
// 3. RemoveClipAction
// ---------------------------------------------------------------------------

pub struct RemoveClipAction {
    pub clip: ClipState,
}

impl UndoAction<AppData> for RemoveClipAction {
    fn description(&self) -> &str {
        "Remove Clip"
    }

    fn apply(&self, state: &mut AppData) {
        state.clips.retain(|c| c.id != self.clip.id);
    }

    fn revert(&self, state: &mut AppData) {
        state.clips.push(self.clip.clone());
    }
}

// ---------------------------------------------------------------------------
// 4. SplitClipAction
// ---------------------------------------------------------------------------

pub struct SplitClipAction {
    pub original_clip: ClipState,
    pub left_clip: ClipState,
    pub right_clip: ClipState,
}

impl UndoAction<AppData> for SplitClipAction {
    fn description(&self) -> &str {
        "Split Clip"
    }

    fn apply(&self, state: &mut AppData) {
        state.clips.retain(|c| c.id != self.original_clip.id);
        state.clips.push(self.left_clip.clone());
        state.clips.push(self.right_clip.clone());
    }

    fn revert(&self, state: &mut AppData) {
        state
            .clips
            .retain(|c| c.id != self.left_clip.id && c.id != self.right_clip.id);
        state.clips.push(self.original_clip.clone());
    }
}

// ---------------------------------------------------------------------------
// 5. DuplicateClipAction
// ---------------------------------------------------------------------------

pub struct DuplicateClipAction {
    pub new_clip: ClipState,
}

impl UndoAction<AppData> for DuplicateClipAction {
    fn description(&self) -> &str {
        "Duplicate Clip"
    }

    fn apply(&self, state: &mut AppData) {
        state.clips.push(self.new_clip.clone());
    }

    fn revert(&self, state: &mut AppData) {
        state.clips.retain(|c| c.id != self.new_clip.id);
    }
}

// ---------------------------------------------------------------------------
// 6. AddNoteAction
// ---------------------------------------------------------------------------

pub struct AddNoteAction {
    pub clip_id: ClipId,
    pub note: Note,
}

impl UndoAction<AppData> for AddNoteAction {
    fn description(&self) -> &str {
        "Add Note"
    }

    fn apply(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let new_clip = clip.with_note_added(self.note);
            state.update_clip(new_clip);
            state.send_command(EngineCommand::AddNote {
                clip_id: self.clip_id,
                note: self.note,
            });
        }
    }

    fn revert(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let new_clip = clip.with_note_removed(self.note.id);
            state.update_clip(new_clip);
            state.send_command(EngineCommand::RemoveNote {
                clip_id: self.clip_id,
                note_id: self.note.id,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// 7. RemoveNoteAction
// ---------------------------------------------------------------------------

pub struct RemoveNoteAction {
    pub clip_id: ClipId,
    pub note: Note,
}

impl UndoAction<AppData> for RemoveNoteAction {
    fn description(&self) -> &str {
        "Remove Note"
    }

    fn apply(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let new_clip = clip.with_note_removed(self.note.id);
            state.update_clip(new_clip);
            state.send_command(EngineCommand::RemoveNote {
                clip_id: self.clip_id,
                note_id: self.note.id,
            });
        }
    }

    fn revert(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let new_clip = clip.with_note_added(self.note);
            state.update_clip(new_clip);
            state.send_command(EngineCommand::AddNote {
                clip_id: self.clip_id,
                note: self.note,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// 8. MoveNoteAction
// ---------------------------------------------------------------------------

pub struct MoveNoteAction {
    pub clip_id: ClipId,
    pub note_id: NoteId,
    pub old_start: Tick,
    pub old_pitch: u8,
    pub new_start: Tick,
    pub new_pitch: u8,
}

impl MoveNoteAction {
    fn apply_values(&self, state: &mut AppData, start: Tick, pitch: u8) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            if let Some(note) = clip.notes.iter().find(|n| n.id == self.note_id) {
                let updated = Note {
                    start_tick: start,
                    pitch,
                    ..*note
                };
                let new_clip = clip.with_note_updated(updated);
                state.update_clip(new_clip);
                state.send_command(EngineCommand::MoveNote {
                    clip_id: self.clip_id,
                    note_id: self.note_id,
                    new_start: start,
                    new_pitch: pitch,
                });
            }
        }
    }
}

impl UndoAction<AppData> for MoveNoteAction {
    fn description(&self) -> &str {
        "Move Note"
    }

    fn apply(&self, state: &mut AppData) {
        self.apply_values(state, self.new_start, self.new_pitch);
    }

    fn revert(&self, state: &mut AppData) {
        self.apply_values(state, self.old_start, self.old_pitch);
    }
}

// ---------------------------------------------------------------------------
// 9. ResizeNoteAction
// ---------------------------------------------------------------------------

pub struct ResizeNoteAction {
    pub clip_id: ClipId,
    pub note_id: NoteId,
    pub old_duration: Tick,
    pub new_duration: Tick,
}

impl ResizeNoteAction {
    fn apply_duration(&self, state: &mut AppData, duration: Tick) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            if let Some(note) = clip.notes.iter().find(|n| n.id == self.note_id) {
                let updated = Note {
                    duration_ticks: duration,
                    ..*note
                };
                let new_clip = clip.with_note_updated(updated);
                state.update_clip(new_clip);
                state.send_command(EngineCommand::ResizeNote {
                    clip_id: self.clip_id,
                    note_id: self.note_id,
                    new_duration: duration,
                });
            }
        }
    }
}

impl UndoAction<AppData> for ResizeNoteAction {
    fn description(&self) -> &str {
        "Resize Note"
    }

    fn apply(&self, state: &mut AppData) {
        self.apply_duration(state, self.new_duration);
    }

    fn revert(&self, state: &mut AppData) {
        self.apply_duration(state, self.old_duration);
    }
}

// ---------------------------------------------------------------------------
// 10. SetTrackVolumeAction
// ---------------------------------------------------------------------------

pub struct SetTrackVolumeAction {
    pub track_id: TrackId,
    pub old_volume: f32,
    pub new_volume: f32,
}

impl SetTrackVolumeAction {
    fn apply_volume(&self, state: &mut AppData, volume: f32) {
        if let Some(track) = state.tracks.iter_mut().find(|t| t.id == self.track_id) {
            track.volume = volume;
        }
        state.send_command(EngineCommand::SetTrackVolume {
            track_id: self.track_id,
            volume,
        });
    }
}

impl UndoAction<AppData> for SetTrackVolumeAction {
    fn description(&self) -> &str {
        "Set Track Volume"
    }

    fn apply(&self, state: &mut AppData) {
        self.apply_volume(state, self.new_volume);
    }

    fn revert(&self, state: &mut AppData) {
        self.apply_volume(state, self.old_volume);
    }
}

// ---------------------------------------------------------------------------
// 11. AddTrackAction
// ---------------------------------------------------------------------------

pub struct AddTrackAction {
    pub track: TrackState,
}

impl UndoAction<AppData> for AddTrackAction {
    fn description(&self) -> &str {
        "Add Track"
    }

    fn apply(&self, state: &mut AppData) {
        state.tracks.push(self.track.clone());
    }

    fn revert(&self, state: &mut AppData) {
        state.tracks.retain(|t| t.id != self.track.id);
    }
}

// ---------------------------------------------------------------------------
// 12. RemoveTrackAction
// ---------------------------------------------------------------------------

pub struct RemoveTrackAction {
    pub track: TrackState,
    pub clips: Vec<ClipState>,
    pub track_index: usize,
}

impl UndoAction<AppData> for RemoveTrackAction {
    fn description(&self) -> &str {
        "Remove Track"
    }

    fn apply(&self, state: &mut AppData) {
        state.tracks.retain(|t| t.id != self.track.id);
        state.clips.retain(|c| c.track_id != self.track.id);
    }

    fn revert(&self, state: &mut AppData) {
        let idx = self.track_index.min(state.tracks.len());
        state.tracks.insert(idx, self.track.clone());
        for clip in &self.clips {
            state.clips.push(clip.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// 13. RenameTrackAction
// ---------------------------------------------------------------------------

pub struct RenameTrackAction {
    pub track_id: TrackId,
    pub old_name: String,
    pub new_name: String,
}

impl UndoAction<AppData> for RenameTrackAction {
    fn description(&self) -> &str {
        "Rename Track"
    }

    fn apply(&self, state: &mut AppData) {
        if let Some(track) = state.tracks.iter_mut().find(|t| t.id == self.track_id) {
            track.name = self.new_name.clone();
        }
    }

    fn revert(&self, state: &mut AppData) {
        if let Some(track) = state.tracks.iter_mut().find(|t| t.id == self.track_id) {
            track.name = self.old_name.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// 14. TransposeNotesAction
// ---------------------------------------------------------------------------

pub struct TransposeNotesAction {
    pub clip_id: ClipId,
    pub note_ids: Vec<NoteId>,
    pub semitones: i8,
}

impl TransposeNotesAction {
    fn transpose(&self, state: &mut AppData, semitones: i8) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let mut updated_clip = clip.clone();
            for note in &mut updated_clip.notes {
                if self.note_ids.contains(&note.id) {
                    note.pitch = (note.pitch as i16 + semitones as i16).clamp(0, 127) as u8;
                }
            }
            state.update_clip(updated_clip);
        }
    }
}

impl UndoAction<AppData> for TransposeNotesAction {
    fn description(&self) -> &str {
        "Transpose Notes"
    }

    fn apply(&self, state: &mut AppData) {
        self.transpose(state, self.semitones);
    }

    fn revert(&self, state: &mut AppData) {
        self.transpose(state, -self.semitones);
    }
}

// ---------------------------------------------------------------------------
// 15. QuantizeNotesAction
// ---------------------------------------------------------------------------

pub struct QuantizeNotesAction {
    pub clip_id: ClipId,
    pub original_starts: Vec<(NoteId, Tick)>,
    pub quantize_grid: QuantizeGrid,
}

impl UndoAction<AppData> for QuantizeNotesAction {
    fn description(&self) -> &str {
        "Quantize Notes"
    }

    fn apply(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let mut updated_clip = clip.clone();
            for note in &mut updated_clip.notes {
                if self.original_starts.iter().any(|(id, _)| *id == note.id) {
                    note.start_tick = self.quantize_grid.snap(note.start_tick);
                }
            }
            updated_clip.notes.sort_by_key(|n| n.start_tick);
            state.update_clip(updated_clip);
        }
    }

    fn revert(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let mut updated_clip = clip.clone();
            for note in &mut updated_clip.notes {
                if let Some((_, original_start)) =
                    self.original_starts.iter().find(|(id, _)| *id == note.id)
                {
                    note.start_tick = *original_start;
                }
            }
            updated_clip.notes.sort_by_key(|n| n.start_tick);
            state.update_clip(updated_clip);
        }
    }
}
