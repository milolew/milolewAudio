//! Selection state and rubber-band hit testing for arrangement clips.

use std::collections::HashSet;

use crate::types::time::Tick;
use crate::types::track::{ClipId, ClipState, TrackId};

/// Axis-aligned bounding box in (tick, track_index) space for rubber-band selection.
pub struct SelectionRect {
    pub tick_start: Tick,
    pub tick_end: Tick,
    pub track_start: usize,
    pub track_end: usize,
}

/// Find all clips that overlap a rubber-band rect.
///
/// Pure function — no UI dependency, fully testable.
/// `track_map` maps each `TrackId` to its index in the track list.
pub fn clips_in_rect(
    clips: &[ClipState],
    track_map: &[(TrackId, usize)],
    rect: &SelectionRect,
) -> HashSet<ClipId> {
    let mut result = HashSet::new();
    for clip in clips {
        let clip_end = clip.start_tick + clip.duration_ticks;

        // Check tick overlap: [clip.start_tick, clip_end) ∩ [rect.tick_start, rect.tick_end)
        if clip_end <= rect.tick_start || clip.start_tick >= rect.tick_end {
            continue;
        }

        // Check track overlap
        if let Some((_, idx)) = track_map.iter().find(|(tid, _)| *tid == clip.track_id) {
            if *idx >= rect.track_start && *idx <= rect.track_end {
                result.insert(clip.id);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn tid(n: u128) -> TrackId {
        TrackId(Uuid::from_u128(n))
    }

    fn cid(n: u128) -> ClipId {
        ClipId(Uuid::from_u128(n))
    }

    fn make_clip(id: u128, track: u128, start: Tick, duration: Tick) -> ClipState {
        ClipState {
            id: cid(id),
            track_id: tid(track),
            start_tick: start,
            duration_ticks: duration,
            name: String::new(),
            notes: Vec::new(),
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        }
    }

    #[test]
    fn rect_selects_overlapping_clip() {
        let clips = vec![make_clip(1, 10, 0, 960)];
        let track_map = vec![(tid(10), 0)];
        let rect = SelectionRect {
            tick_start: 100,
            tick_end: 500,
            track_start: 0,
            track_end: 0,
        };
        let result = clips_in_rect(&clips, &track_map, &rect);
        assert!(result.contains(&cid(1)));
    }

    #[test]
    fn rect_misses_clip_on_different_track() {
        let clips = vec![make_clip(1, 10, 0, 960)];
        let track_map = vec![(tid(10), 0), (tid(20), 1)];
        let rect = SelectionRect {
            tick_start: 0,
            tick_end: 960,
            track_start: 1,
            track_end: 1,
        };
        let result = clips_in_rect(&clips, &track_map, &rect);
        assert!(result.is_empty());
    }

    #[test]
    fn rect_misses_clip_outside_tick_range() {
        let clips = vec![make_clip(1, 10, 1000, 960)];
        let track_map = vec![(tid(10), 0)];
        let rect = SelectionRect {
            tick_start: 0,
            tick_end: 500,
            track_start: 0,
            track_end: 0,
        };
        let result = clips_in_rect(&clips, &track_map, &rect);
        assert!(result.is_empty());
    }

    #[test]
    fn rect_selects_multiple_clips() {
        let clips = vec![
            make_clip(1, 10, 0, 960),
            make_clip(2, 10, 960, 960),
            make_clip(3, 20, 0, 960),
        ];
        let track_map = vec![(tid(10), 0), (tid(20), 1)];
        let rect = SelectionRect {
            tick_start: 0,
            tick_end: 2000,
            track_start: 0,
            track_end: 1,
        };
        let result = clips_in_rect(&clips, &track_map, &rect);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn rect_boundary_no_overlap() {
        // Clip ends exactly at rect start — no overlap
        let clips = vec![make_clip(1, 10, 0, 960)];
        let track_map = vec![(tid(10), 0)];
        let rect = SelectionRect {
            tick_start: 960,
            tick_end: 1920,
            track_start: 0,
            track_end: 0,
        };
        let result = clips_in_rect(&clips, &track_map, &rect);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_clips_returns_empty() {
        let clips: Vec<ClipState> = Vec::new();
        let track_map = vec![(tid(10), 0)];
        let rect = SelectionRect {
            tick_start: 0,
            tick_end: 960,
            track_start: 0,
            track_end: 0,
        };
        let result = clips_in_rect(&clips, &track_map, &rect);
        assert!(result.is_empty());
    }
}
