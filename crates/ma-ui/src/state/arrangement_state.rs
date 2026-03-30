//! Arrangement view state — zoom, scroll, selection, interaction, snap.

use std::collections::HashSet;

use crate::types::track::{ClipId, TrackId};
use crate::views::arrangement::clip_interaction::ClipInteraction;
use crate::views::arrangement::clipboard::ClipClipboard;
use crate::views::arrangement::snap::SnapGrid;

/// Immutable clip selection state backed by a HashSet for O(1) contains().
#[derive(Debug, Clone, Default)]
pub struct ClipSelection {
    pub clips: HashSet<ClipId>,
}

impl ClipSelection {
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    pub fn contains(&self, id: &ClipId) -> bool {
        self.clips.contains(id)
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    /// Create a selection with a single clip.
    pub fn select_single(id: ClipId) -> Self {
        let mut clips = HashSet::new();
        clips.insert(id);
        Self { clips }
    }

    /// Toggle a clip in/out of selection (for Shift+click).
    pub fn toggled(&self, id: ClipId) -> Self {
        let mut clips = self.clips.clone();
        if !clips.remove(&id) {
            clips.insert(id);
        }
        Self { clips }
    }
}

#[derive(Debug, Clone)]
pub struct ArrangementState {
    /// Horizontal zoom: pixels per tick.
    pub zoom_x: f64,
    /// Horizontal scroll offset in ticks.
    pub scroll_x: f64,
    /// Vertical scroll offset in pixels.
    pub scroll_y: f32,
    /// Currently selected track.
    pub selected_track: Option<TrackId>,
    /// Currently selected clips.
    pub selected_clips: ClipSelection,
    /// Track lane height in pixels.
    pub track_height: f32,
    /// Arrangement snap grid.
    pub snap_grid: SnapGrid,
    /// Current mouse interaction state.
    pub interaction: ClipInteraction,
    /// Clipboard for copy/paste.
    pub clipboard: ClipClipboard,
    /// Auto-scroll to keep the playhead visible during playback.
    pub follow_playhead: bool,
}

impl Default for ArrangementState {
    fn default() -> Self {
        Self {
            zoom_x: 0.05,
            scroll_x: 0.0,
            scroll_y: 0.0,
            selected_track: None,
            selected_clips: ClipSelection::default(),
            track_height: 80.0,
            snap_grid: SnapGrid::default(),
            interaction: ClipInteraction::default(),
            clipboard: ClipClipboard::default(),
            follow_playhead: true,
        }
    }
}

impl ArrangementState {
    /// Convert a tick position to x pixel coordinate.
    pub fn tick_to_x(&self, tick: i64) -> f32 {
        ((tick as f64 - self.scroll_x) * self.zoom_x) as f32
    }

    /// Convert an x pixel coordinate to tick position.
    pub fn x_to_tick(&self, x: f32) -> i64 {
        ((x as f64 / self.zoom_x) + self.scroll_x) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn cid(n: u128) -> ClipId {
        ClipId(Uuid::from_u128(n))
    }

    #[test]
    fn selection_single() {
        let sel = ClipSelection::select_single(cid(1));
        assert!(sel.contains(&cid(1)));
        assert!(!sel.contains(&cid(2)));
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn selection_toggle_add() {
        let sel = ClipSelection::select_single(cid(1));
        let sel2 = sel.toggled(cid(2));
        assert!(sel2.contains(&cid(1)));
        assert!(sel2.contains(&cid(2)));
        assert_eq!(sel2.len(), 2);
    }

    #[test]
    fn selection_toggle_remove() {
        let sel = ClipSelection::select_single(cid(1));
        let sel2 = sel.toggled(cid(1));
        assert!(sel2.is_empty());
    }

    #[test]
    fn selection_default_empty() {
        let sel = ClipSelection::default();
        assert!(sel.is_empty());
        assert_eq!(sel.len(), 0);
    }
}
