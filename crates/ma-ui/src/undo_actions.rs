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
            state.send_command(EngineCommand::SetTrackVolume {
                track_id: self.track_id,
                volume,
            });
        }
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
    pub original_pitches: Vec<(NoteId, u8)>,
    pub semitones: i8,
}

impl UndoAction<AppData> for TransposeNotesAction {
    fn description(&self) -> &str {
        "Transpose Notes"
    }

    fn apply(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let mut updated_clip = clip.clone();
            for note in &mut updated_clip.notes {
                if self.original_pitches.iter().any(|(id, _)| *id == note.id) {
                    note.pitch = (note.pitch as i16 + self.semitones as i16).clamp(0, 127) as u8;
                    state.send_command(EngineCommand::MoveNote {
                        clip_id: self.clip_id,
                        note_id: note.id,
                        new_start: note.start_tick,
                        new_pitch: note.pitch,
                    });
                }
            }
            state.update_clip(updated_clip);
        }
    }

    fn revert(&self, state: &mut AppData) {
        if let Some(clip) = state.clips.iter().find(|c| c.id == self.clip_id) {
            let mut updated_clip = clip.clone();
            for note in &mut updated_clip.notes {
                if let Some((_, original_pitch)) =
                    self.original_pitches.iter().find(|(id, _)| *id == note.id)
                {
                    note.pitch = *original_pitch;
                    state.send_command(EngineCommand::MoveNote {
                        clip_id: self.clip_id,
                        note_id: note.id,
                        new_start: note.start_tick,
                        new_pitch: note.pitch,
                    });
                }
            }
            state.update_clip(updated_clip);
        }
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
                    state.send_command(EngineCommand::MoveNote {
                        clip_id: self.clip_id,
                        note_id: note.id,
                        new_start: note.start_tick,
                        new_pitch: note.pitch,
                    });
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
                    state.send_command(EngineCommand::MoveNote {
                        clip_id: self.clip_id,
                        note_id: note.id,
                        new_start: note.start_tick,
                        new_pitch: note.pitch,
                    });
                }
            }
            updated_clip.notes.sort_by_key(|n| n.start_tick);
            state.update_clip(updated_clip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::midi::NoteId;
    use crate::types::time::PPQN;
    use ma_core::undo::UndoManager;

    /// Helper: deterministic UUID from u64.
    fn demo_id(n: u64) -> uuid::Uuid {
        uuid::Uuid::from_u64_pair(0, n)
    }

    /// Clip ID from demo data.
    fn clip_id(n: u64) -> ClipId {
        ClipId(demo_id(n))
    }

    /// Track ID from demo data.
    fn track_id(n: u64) -> TrackId {
        TrackId(demo_id(n))
    }

    fn make_note(id: u64, pitch: u8, start: Tick, dur: Tick) -> Note {
        Note {
            id: NoteId(id),
            pitch,
            start_tick: start,
            duration_ticks: dur,
            velocity: 100,
            channel: 0,
        }
    }

    // -- MoveClipAction --

    #[test]
    fn move_clip_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);
        let original_start = state.clip(cid).unwrap().start_tick;

        let action = MoveClipAction {
            clip_id: cid,
            old_start_tick: original_start,
            new_start_tick: PPQN * 16,
        };

        action.apply(&mut state);
        assert_eq!(state.clip(cid).unwrap().start_tick, PPQN * 16);

        action.revert(&mut state);
        assert_eq!(state.clip(cid).unwrap().start_tick, original_start);
    }

    // -- AddClipAction --

    #[test]
    fn add_clip_apply_and_revert() {
        let mut state = AppData::new();
        let initial_count = state.clips.len();
        let new_clip = ClipState {
            id: ClipId(demo_id(99)),
            track_id: track_id(1),
            start_tick: 0,
            duration_ticks: PPQN * 4,
            name: "Test Clip".into(),
            notes: Vec::new(),
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        };

        let action = AddClipAction {
            clip: new_clip.clone(),
        };

        action.apply(&mut state);
        assert_eq!(state.clips.len(), initial_count + 1);
        assert!(state.clip(ClipId(demo_id(99))).is_some());

        action.revert(&mut state);
        assert_eq!(state.clips.len(), initial_count);
        assert!(state.clip(ClipId(demo_id(99))).is_none());
    }

    // -- RemoveClipAction --

    #[test]
    fn remove_clip_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);
        let clip = state.clip(cid).unwrap().clone();
        let initial_count = state.clips.len();

        let action = RemoveClipAction { clip };

        action.apply(&mut state);
        assert_eq!(state.clips.len(), initial_count - 1);
        assert!(state.clip(cid).is_none());

        action.revert(&mut state);
        assert_eq!(state.clips.len(), initial_count);
        assert!(state.clip(cid).is_some());
    }

    // -- SplitClipAction --

    #[test]
    fn split_clip_apply_and_revert() {
        let mut state = AppData::new();
        let original = state.clip(clip_id(1)).unwrap().clone();
        let initial_count = state.clips.len();

        let left = ClipState {
            id: ClipId(demo_id(50)),
            duration_ticks: PPQN * 4,
            name: "Left".into(),
            ..original.clone()
        };
        let right = ClipState {
            id: ClipId(demo_id(51)),
            start_tick: PPQN * 4,
            duration_ticks: PPQN * 4,
            name: "Right".into(),
            ..original.clone()
        };

        let action = SplitClipAction {
            original_clip: original,
            left_clip: left,
            right_clip: right,
        };

        action.apply(&mut state);
        assert!(state.clip(clip_id(1)).is_none());
        assert!(state.clip(ClipId(demo_id(50))).is_some());
        assert!(state.clip(ClipId(demo_id(51))).is_some());
        assert_eq!(state.clips.len(), initial_count + 1); // -1 original +2 halves

        action.revert(&mut state);
        assert!(state.clip(clip_id(1)).is_some());
        assert!(state.clip(ClipId(demo_id(50))).is_none());
        assert!(state.clip(ClipId(demo_id(51))).is_none());
        assert_eq!(state.clips.len(), initial_count);
    }

    // -- DuplicateClipAction --

    #[test]
    fn duplicate_clip_apply_and_revert() {
        let mut state = AppData::new();
        let source = state.clip(clip_id(1)).unwrap().clone();
        let new_clip = ClipState {
            id: ClipId(demo_id(60)),
            start_tick: source.end_tick(),
            ..source
        };
        let initial_count = state.clips.len();

        let action = DuplicateClipAction {
            new_clip: new_clip.clone(),
        };

        action.apply(&mut state);
        assert_eq!(state.clips.len(), initial_count + 1);
        assert!(state.clip(ClipId(demo_id(60))).is_some());

        action.revert(&mut state);
        assert_eq!(state.clips.len(), initial_count);
        assert!(state.clip(ClipId(demo_id(60))).is_none());
    }

    // -- AddNoteAction --

    #[test]
    fn add_note_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);
        let initial_note_count = state.clip(cid).unwrap().notes.len();
        let note = make_note(999, 72, PPQN * 4, PPQN);

        let action = AddNoteAction { clip_id: cid, note };

        action.apply(&mut state);
        assert_eq!(state.clip(cid).unwrap().notes.len(), initial_note_count + 1);

        action.revert(&mut state);
        assert_eq!(state.clip(cid).unwrap().notes.len(), initial_note_count);
    }

    // -- RemoveNoteAction --

    #[test]
    fn remove_note_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);
        let note = *state.clip(cid).unwrap().notes.first().unwrap();
        let initial_note_count = state.clip(cid).unwrap().notes.len();

        let action = RemoveNoteAction { clip_id: cid, note };

        action.apply(&mut state);
        assert_eq!(state.clip(cid).unwrap().notes.len(), initial_note_count - 1);

        action.revert(&mut state);
        assert_eq!(state.clip(cid).unwrap().notes.len(), initial_note_count);
    }

    // -- MoveNoteAction --

    #[test]
    fn move_note_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);
        let note = *state.clip(cid).unwrap().notes.first().unwrap();

        let action = MoveNoteAction {
            clip_id: cid,
            note_id: note.id,
            old_start: note.start_tick,
            old_pitch: note.pitch,
            new_start: PPQN * 8,
            new_pitch: 80,
        };

        action.apply(&mut state);
        let moved = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap();
        assert_eq!(moved.start_tick, PPQN * 8);
        assert_eq!(moved.pitch, 80);

        action.revert(&mut state);
        let restored = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap();
        assert_eq!(restored.start_tick, note.start_tick);
        assert_eq!(restored.pitch, note.pitch);
    }

    // -- ResizeNoteAction --

    #[test]
    fn resize_note_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);
        let note = *state.clip(cid).unwrap().notes.first().unwrap();

        let action = ResizeNoteAction {
            clip_id: cid,
            note_id: note.id,
            old_duration: note.duration_ticks,
            new_duration: PPQN * 4,
        };

        action.apply(&mut state);
        let resized = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap();
        assert_eq!(resized.duration_ticks, PPQN * 4);

        action.revert(&mut state);
        let restored = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap();
        assert_eq!(restored.duration_ticks, note.duration_ticks);
    }

    // -- SetTrackVolumeAction --

    #[test]
    fn set_track_volume_apply_and_revert() {
        let mut state = AppData::new();
        let tid = track_id(1);
        let original_vol = state.track(tid).unwrap().volume;

        let action = SetTrackVolumeAction {
            track_id: tid,
            old_volume: original_vol,
            new_volume: 0.3,
        };

        action.apply(&mut state);
        assert!((state.track(tid).unwrap().volume - 0.3).abs() < f32::EPSILON);

        action.revert(&mut state);
        assert!((state.track(tid).unwrap().volume - original_vol).abs() < f32::EPSILON);
    }

    // -- AddTrackAction --

    #[test]
    fn add_track_apply_and_revert() {
        let mut state = AppData::new();
        let initial_count = state.tracks.len();
        let new_track = TrackState::new_midi(TrackId(demo_id(77)), "New Track", [0, 255, 0]);

        let action = AddTrackAction {
            track: new_track.clone(),
        };

        action.apply(&mut state);
        assert_eq!(state.tracks.len(), initial_count + 1);
        assert!(state.track(TrackId(demo_id(77))).is_some());

        action.revert(&mut state);
        assert_eq!(state.tracks.len(), initial_count);
        assert!(state.track(TrackId(demo_id(77))).is_none());
    }

    // -- RemoveTrackAction --

    #[test]
    fn remove_track_restores_clips() {
        let mut state = AppData::new();
        let tid = track_id(1);
        let track = state.track(tid).unwrap().clone();
        let track_clips: Vec<ClipState> = state
            .clips
            .iter()
            .filter(|c| c.track_id == tid)
            .cloned()
            .collect();
        let track_idx = state.tracks.iter().position(|t| t.id == tid).unwrap();
        let initial_tracks = state.tracks.len();
        let initial_clips = state.clips.len();
        let track_clip_count = track_clips.len();

        let action = RemoveTrackAction {
            track,
            clips: track_clips,
            track_index: track_idx,
        };

        action.apply(&mut state);
        assert_eq!(state.tracks.len(), initial_tracks - 1);
        assert!(state.track(tid).is_none());
        assert_eq!(state.clips.len(), initial_clips - track_clip_count);

        action.revert(&mut state);
        assert_eq!(state.tracks.len(), initial_tracks);
        assert!(state.track(tid).is_some());
        assert_eq!(state.clips.len(), initial_clips);
    }

    // -- RenameTrackAction --

    #[test]
    fn rename_track_apply_and_revert() {
        let mut state = AppData::new();
        let tid = track_id(1);
        let old_name = state.track(tid).unwrap().name.clone();

        let action = RenameTrackAction {
            track_id: tid,
            old_name: old_name.clone(),
            new_name: "Renamed".into(),
        };

        action.apply(&mut state);
        assert_eq!(state.track(tid).unwrap().name, "Renamed");

        action.revert(&mut state);
        assert_eq!(state.track(tid).unwrap().name, old_name);
    }

    // -- TransposeNotesAction --

    #[test]
    fn transpose_notes_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);
        let original_pitches: Vec<(NoteId, u8)> = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .map(|n| (n.id, n.pitch))
            .collect();

        let action = TransposeNotesAction {
            clip_id: cid,
            original_pitches: original_pitches.clone(),
            semitones: 5,
        };

        action.apply(&mut state);
        for (nid, orig_pitch) in &original_pitches {
            let note = state
                .clip(cid)
                .unwrap()
                .notes
                .iter()
                .find(|n| n.id == *nid)
                .unwrap();
            assert_eq!(note.pitch, orig_pitch + 5);
        }

        action.revert(&mut state);
        for (nid, orig_pitch) in &original_pitches {
            let note = state
                .clip(cid)
                .unwrap()
                .notes
                .iter()
                .find(|n| n.id == *nid)
                .unwrap();
            assert_eq!(note.pitch, *orig_pitch);
        }
    }

    #[test]
    fn transpose_notes_clamp_boundary_is_invertible() {
        let mut state = AppData::new();
        let cid = clip_id(1);

        // Set a note to pitch 125 — close to 127 boundary
        let clip = state.clip(cid).unwrap().clone();
        let note_id = clip.notes[0].id;
        let mut modified = clip.clone();
        modified.notes[0].pitch = 125;
        state.update_clip(modified);

        let original_pitches = vec![(note_id, 125u8)];

        let action = TransposeNotesAction {
            clip_id: cid,
            original_pitches,
            semitones: 5,
        };

        action.apply(&mut state);
        let note = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == note_id)
            .unwrap();
        assert_eq!(note.pitch, 127); // clamped

        // Revert must restore exact original, not 127-5=122
        action.revert(&mut state);
        let note = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == note_id)
            .unwrap();
        assert_eq!(note.pitch, 125); // exact restore
    }

    // -- QuantizeNotesAction --

    #[test]
    fn quantize_notes_apply_and_revert() {
        let mut state = AppData::new();
        let cid = clip_id(1);

        // Shift notes to non-quantized positions
        let clip = state.clip(cid).unwrap().clone();
        let mut modified = clip.clone();
        modified.notes[0].start_tick = 100; // off-grid
        modified.notes[1].start_tick = 550; // off-grid
        state.update_clip(modified);

        let original_starts: Vec<(NoteId, Tick)> = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .map(|n| (n.id, n.start_tick))
            .collect();

        let action = QuantizeNotesAction {
            clip_id: cid,
            original_starts: original_starts.clone(),
            quantize_grid: QuantizeGrid::Quarter,
        };

        action.apply(&mut state);
        let note0 = state
            .clip(cid)
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == original_starts[0].0)
            .unwrap();
        assert_eq!(note0.start_tick, QuantizeGrid::Quarter.snap(100));

        action.revert(&mut state);
        for (nid, orig_start) in &original_starts {
            let note = state
                .clip(cid)
                .unwrap()
                .notes
                .iter()
                .find(|n| n.id == *nid)
                .unwrap();
            assert_eq!(note.start_tick, *orig_start);
        }
    }

    // -- Full cycle: push → undo → redo --

    #[test]
    fn full_undo_redo_cycle_with_manager() {
        let mut state = AppData::new();
        let mut mgr: UndoManager<AppData> = UndoManager::new(100);
        let cid = clip_id(1);
        let original_start = state.clip(cid).unwrap().start_tick;

        // Apply and push
        let action = MoveClipAction {
            clip_id: cid,
            old_start_tick: original_start,
            new_start_tick: PPQN * 32,
        };
        action.apply(&mut state);
        mgr.push(Box::new(action));
        assert_eq!(state.clip(cid).unwrap().start_tick, PPQN * 32);

        // Undo
        mgr.undo(&mut state);
        assert_eq!(state.clip(cid).unwrap().start_tick, original_start);

        // Redo
        mgr.redo(&mut state);
        assert_eq!(state.clip(cid).unwrap().start_tick, PPQN * 32);
    }

    #[test]
    fn descriptions_are_non_empty() {
        let actions: Vec<Box<dyn UndoAction<AppData>>> = vec![
            Box::new(MoveClipAction {
                clip_id: clip_id(1),
                old_start_tick: 0,
                new_start_tick: 100,
            }),
            Box::new(AddClipAction {
                clip: ClipState {
                    id: clip_id(99),
                    track_id: track_id(1),
                    start_tick: 0,
                    duration_ticks: 100,
                    name: String::new(),
                    notes: Vec::new(),
                    audio_file: None,
                    audio_length_samples: None,
                    audio_sample_rate: None,
                },
            }),
            Box::new(RemoveClipAction {
                clip: ClipState {
                    id: clip_id(99),
                    track_id: track_id(1),
                    start_tick: 0,
                    duration_ticks: 100,
                    name: String::new(),
                    notes: Vec::new(),
                    audio_file: None,
                    audio_length_samples: None,
                    audio_sample_rate: None,
                },
            }),
            Box::new(AddNoteAction {
                clip_id: clip_id(1),
                note: make_note(1, 60, 0, 100),
            }),
            Box::new(RemoveNoteAction {
                clip_id: clip_id(1),
                note: make_note(1, 60, 0, 100),
            }),
            Box::new(MoveNoteAction {
                clip_id: clip_id(1),
                note_id: NoteId(1),
                old_start: 0,
                old_pitch: 60,
                new_start: 100,
                new_pitch: 72,
            }),
            Box::new(ResizeNoteAction {
                clip_id: clip_id(1),
                note_id: NoteId(1),
                old_duration: 100,
                new_duration: 200,
            }),
            Box::new(SetTrackVolumeAction {
                track_id: track_id(1),
                old_volume: 0.8,
                new_volume: 0.5,
            }),
            Box::new(AddTrackAction {
                track: TrackState::new_midi(TrackId(demo_id(88)), "T", [0, 0, 0]),
            }),
            Box::new(RemoveTrackAction {
                track: TrackState::new_midi(TrackId(demo_id(88)), "T", [0, 0, 0]),
                clips: Vec::new(),
                track_index: 0,
            }),
            Box::new(RenameTrackAction {
                track_id: track_id(1),
                old_name: "A".into(),
                new_name: "B".into(),
            }),
            Box::new(TransposeNotesAction {
                clip_id: clip_id(1),
                original_pitches: vec![(NoteId(100), 60)],
                semitones: 3,
            }),
            Box::new(QuantizeNotesAction {
                clip_id: clip_id(1),
                original_starts: vec![(NoteId(100), 0)],
                quantize_grid: QuantizeGrid::Quarter,
            }),
        ];

        for action in &actions {
            assert!(
                !action.description().is_empty(),
                "Action description should not be empty"
            );
        }
    }
}
