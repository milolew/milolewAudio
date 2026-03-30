//! Arrangement view state — zoom, scroll, selection.

use crate::types::track::{ClipId, TrackId};

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
    pub selected_clips: Vec<ClipId>,
    /// Track lane height in pixels.
    pub track_height: f32,
    /// Track being renamed inline (None = not editing).
    pub editing_track: Option<TrackId>,
    /// Buffer for the name being edited.
    pub editing_name: String,
}

impl Default for ArrangementState {
    fn default() -> Self {
        Self {
            zoom_x: 0.05,
            scroll_x: 0.0,
            scroll_y: 0.0,
            selected_track: None,
            selected_clips: Vec::new(),
            track_height: 80.0,
            editing_track: None,
            editing_name: String::new(),
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
