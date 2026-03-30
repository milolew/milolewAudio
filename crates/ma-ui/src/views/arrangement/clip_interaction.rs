//! Clip interaction state machine — mouse FSM for arrangement view.
//!
//! States: Idle → PendingDrag → MovingClips / ResizingClip / (click)
//!         Idle → RubberBand → (commit selection)

use crate::types::time::{Tick, PPQN};
use crate::types::track::{ClipId, ClipState, TrackId};

/// Pixel threshold before a click becomes a drag.
pub const DRAG_THRESHOLD: f32 = 4.0;

/// Pixel width of the resize hit zone at clip edges.
pub const EDGE_HIT_ZONE: f32 = 6.0;

/// Minimum clip duration after resize (1/64 note at 960 PPQN).
pub const MIN_CLIP_DURATION: Tick = PPQN / 16;

/// Which edge of a clip is being resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipResizeEdge {
    Left,
    Right,
}

/// Hit-test result: which zone of a clip was clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipHitZone {
    Body,
    LeftEdge,
    RightEdge,
}

/// Clip interaction state machine.
#[derive(Debug, Clone, Default)]
pub enum ClipInteraction {
    #[default]
    Idle,

    /// Click happened, waiting for threshold to distinguish click vs drag.
    PendingDrag {
        clip_id: ClipId,
        track_id: TrackId,
        mouse_start_x: f32,
        mouse_start_y: f32,
        click_tick: Tick,
        hit_zone: ClipHitZone,
    },

    /// Dragging one or more selected clips to a new position.
    MovingClips {
        anchor_clip_id: ClipId,
        anchor_original_start: Tick,
        anchor_original_track: TrackId,
        /// Tick offset from anchor clip start to where the mouse grabbed it.
        grab_offset_tick: Tick,
        /// Current delta applied to all selected clips (for ghost rendering).
        delta_tick: Tick,
        delta_track_index: i32,
    },

    /// Resizing a clip from one edge.
    ResizingClip {
        clip_id: ClipId,
        edge: ClipResizeEdge,
        original_start: Tick,
        original_duration: Tick,
    },

    /// Rubber-band selection rectangle (screen pixel coords).
    RubberBand {
        origin_x: f32,
        origin_y: f32,
        current_x: f32,
        current_y: f32,
    },
}

/// Determine which zone of a clip was hit.
///
/// `rel_x` is the cursor x relative to clip left edge.
/// `clip_w` is the clip width in pixels.
/// Edge zones are only active when clip is wide enough (> 3× edge zone).
pub fn classify_hit_zone(rel_x: f32, clip_w: f32) -> ClipHitZone {
    if clip_w > EDGE_HIT_ZONE * 3.0 {
        if rel_x <= EDGE_HIT_ZONE {
            return ClipHitZone::LeftEdge;
        }
        if rel_x >= clip_w - EDGE_HIT_ZONE {
            return ClipHitZone::RightEdge;
        }
    }
    ClipHitZone::Body
}

/// Hit-test clips at a pixel position, returning clip ID and hit zone.
pub fn hit_test_clip_zone(
    clips: &[ClipState],
    zoom_x: f64,
    scroll_x: f64,
    x: f32,
    bounds_x: f32,
) -> Option<(ClipId, ClipHitZone)> {
    for clip in clips {
        let clip_x = bounds_x + ((clip.start_tick as f64 - scroll_x) * zoom_x) as f32;
        let clip_w = (clip.duration_ticks as f64 * zoom_x) as f32;
        if x >= clip_x && x <= clip_x + clip_w {
            let rel_x = x - clip_x;
            let zone = classify_hit_zone(rel_x, clip_w);
            return Some((clip.id, zone));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_body_center() {
        assert_eq!(classify_hit_zone(30.0, 100.0), ClipHitZone::Body);
    }

    #[test]
    fn classify_left_edge() {
        assert_eq!(classify_hit_zone(3.0, 100.0), ClipHitZone::LeftEdge);
    }

    #[test]
    fn classify_right_edge() {
        assert_eq!(classify_hit_zone(97.0, 100.0), ClipHitZone::RightEdge);
    }

    #[test]
    fn classify_left_edge_boundary() {
        assert_eq!(classify_hit_zone(6.0, 100.0), ClipHitZone::LeftEdge);
    }

    #[test]
    fn classify_right_edge_boundary() {
        assert_eq!(classify_hit_zone(94.0, 100.0), ClipHitZone::RightEdge);
    }

    #[test]
    fn classify_narrow_clip_all_body() {
        // Clip narrower than 3× edge zone — no edge detection
        assert_eq!(classify_hit_zone(1.0, 15.0), ClipHitZone::Body);
        assert_eq!(classify_hit_zone(14.0, 15.0), ClipHitZone::Body);
    }

    #[test]
    fn classify_at_edge_threshold() {
        // Exactly at 3× threshold boundary
        let w = EDGE_HIT_ZONE * 3.0 + 0.01;
        assert_eq!(classify_hit_zone(3.0, w), ClipHitZone::LeftEdge);
    }
}
