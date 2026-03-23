//! Top-level application state.

use crate::types::track::{ClipId, ClipState, TrackId, TrackState};

use super::arrangement_state::ArrangementState;
use super::mixer_state::MixerState;
use super::piano_roll_state::PianoRollState;
use super::transport_state::TransportState;

/// Which main view is currently active (bottom panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Arrangement,
    Mixer,
    PianoRoll,
}

/// The root application state. Owned by DawApp, passed down as &refs.
#[derive(Debug, Clone)]
pub struct AppState {
    pub transport: TransportState,
    pub arrangement: ArrangementState,
    pub mixer: MixerState,
    pub piano_roll: PianoRollState,
    pub tracks: Vec<TrackState>,
    pub clips: Vec<ClipState>,
    pub active_view: ActiveView,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            transport: TransportState::default(),
            arrangement: ArrangementState::default(),
            mixer: MixerState::default(),
            piano_roll: PianoRollState::default(),
            tracks: Vec::new(),
            clips: Vec::new(),
            active_view: ActiveView::Arrangement,
        }
    }
}

impl AppState {
    /// Find a track by ID.
    pub fn track(&self, id: TrackId) -> Option<&TrackState> {
        self.tracks.iter().find(|t| t.id == id)
    }

    /// Find a clip by ID.
    pub fn clip(&self, id: ClipId) -> Option<&ClipState> {
        self.clips.iter().find(|c| c.id == id)
    }

    /// Get clips belonging to a specific track.
    pub fn clips_for_track(&self, track_id: TrackId) -> Vec<&ClipState> {
        self.clips.iter().filter(|c| c.track_id == track_id).collect()
    }

    /// Replace a clip by ID (mutates in-place for efficiency).
    pub fn update_clip(&mut self, updated: ClipState) {
        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == updated.id) {
            *clip = updated;
        }
    }

    /// Replace a track by ID (mutates in-place for efficiency).
    pub fn update_track(&mut self, updated: TrackState) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == updated.id) {
            *track = updated;
        }
    }
}
