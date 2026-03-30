//! Clipboard for arrangement clip copy/paste operations.

use crate::types::time::Tick;
use crate::types::track::{ClipState, TrackId};

/// A clip on the clipboard with its relative position preserved.
#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    /// Full clip data (deep clone; new ID assigned on paste).
    pub clip: ClipState,
    /// Tick offset from the earliest clip in the copied selection.
    pub tick_offset: Tick,
    /// Track index offset from the topmost track in the copied selection.
    pub track_index_offset: i32,
}

/// Clipboard for arrangement clip operations.
#[derive(Debug, Clone, Default)]
pub struct ClipClipboard {
    pub entries: Vec<ClipboardEntry>,
}

impl ClipClipboard {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Build clipboard from selected clips.
    ///
    /// Normalizes offsets so the earliest clip is at tick_offset=0
    /// and the topmost track is at track_index_offset=0.
    pub fn from_clips(clips: &[ClipState], track_map: &[(TrackId, usize)]) -> Self {
        if clips.is_empty() {
            return Self::default();
        }

        let min_tick = clips.iter().map(|c| c.start_tick).min().unwrap_or(0);
        let min_track_idx = clips
            .iter()
            .filter_map(|c| {
                track_map
                    .iter()
                    .find(|(tid, _)| *tid == c.track_id)
                    .map(|(_, i)| *i)
            })
            .min()
            .unwrap_or(0);

        let entries = clips
            .iter()
            .map(|c| {
                let track_idx = track_map
                    .iter()
                    .find(|(tid, _)| *tid == c.track_id)
                    .map(|(_, i)| *i)
                    .unwrap_or(0);
                ClipboardEntry {
                    clip: c.clone(),
                    tick_offset: c.start_tick - min_tick,
                    track_index_offset: track_idx as i32 - min_track_idx as i32,
                }
            })
            .collect();

        Self { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn tid(n: u128) -> TrackId {
        TrackId(Uuid::from_u128(n))
    }

    fn make_clip(id: u128, track: u128, start: Tick, duration: Tick) -> ClipState {
        ClipState {
            id: crate::types::track::ClipId(Uuid::from_u128(id)),
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
    fn clipboard_normalizes_offsets() {
        let clips = vec![make_clip(1, 10, 960, 480), make_clip(2, 20, 1920, 480)];
        let track_map = vec![(tid(10), 0), (tid(20), 1)];
        let clipboard = ClipClipboard::from_clips(&clips, &track_map);

        assert_eq!(clipboard.entries.len(), 2);
        assert_eq!(clipboard.entries[0].tick_offset, 0);
        assert_eq!(clipboard.entries[0].track_index_offset, 0);
        assert_eq!(clipboard.entries[1].tick_offset, 960);
        assert_eq!(clipboard.entries[1].track_index_offset, 1);
    }

    #[test]
    fn clipboard_empty_clips() {
        let clipboard = ClipClipboard::from_clips(&[], &[]);
        assert!(clipboard.is_empty());
    }

    #[test]
    fn clipboard_single_clip() {
        let clips = vec![make_clip(1, 10, 500, 480)];
        let track_map = vec![(tid(10), 2)];
        let clipboard = ClipClipboard::from_clips(&clips, &track_map);

        assert_eq!(clipboard.entries.len(), 1);
        assert_eq!(clipboard.entries[0].tick_offset, 0);
        assert_eq!(clipboard.entries[0].track_index_offset, 0);
    }
}
