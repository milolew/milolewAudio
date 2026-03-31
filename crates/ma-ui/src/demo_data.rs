//! Demo data — deterministic tracks and clips for startup/testing.

use crate::types::midi::{Note, NoteId};
use crate::types::time::PPQN;
use crate::types::track::{ClipId, ClipState, TrackId, TrackState};

/// Create a deterministic UUID for demo data (stable across restarts).
fn demo_id(n: u64) -> uuid::Uuid {
    uuid::Uuid::from_u64_pair(0, n)
}

/// Demo tracks: Melody (MIDI), Bass (MIDI), Drums (Audio), Pad (MIDI).
pub fn demo_tracks() -> Vec<TrackState> {
    vec![
        TrackState::new_midi(TrackId(demo_id(1)), "Melody", [100, 160, 255]),
        TrackState::new_midi(TrackId(demo_id(2)), "Bass", [255, 140, 80]),
        TrackState::new_audio(TrackId(demo_id(3)), "Drums", [80, 220, 120]),
        TrackState::new_midi(TrackId(demo_id(4)), "Pad", [200, 100, 255]),
    ]
}

/// IDs of all demo tracks (for engine initialization).
pub fn demo_track_ids() -> Vec<TrackId> {
    demo_tracks().iter().map(|t| t.id).collect()
}

/// Demo clips with sample MIDI notes.
pub fn demo_clips() -> Vec<ClipState> {
    vec![
        ClipState {
            id: ClipId(demo_id(1)),
            track_id: TrackId(demo_id(1)),
            start_tick: 0,
            duration_ticks: PPQN * 8,
            name: "Melody A".into(),
            notes: vec![
                Note {
                    id: NoteId(100),
                    pitch: 60,
                    start_tick: 0,
                    duration_ticks: PPQN / 2,
                    velocity: 100,
                    channel: 0,
                },
                Note {
                    id: NoteId(101),
                    pitch: 64,
                    start_tick: PPQN / 2,
                    duration_ticks: PPQN / 2,
                    velocity: 90,
                    channel: 0,
                },
                Note {
                    id: NoteId(102),
                    pitch: 67,
                    start_tick: PPQN,
                    duration_ticks: PPQN,
                    velocity: 110,
                    channel: 0,
                },
                Note {
                    id: NoteId(103),
                    pitch: 72,
                    start_tick: PPQN * 2,
                    duration_ticks: PPQN * 2,
                    velocity: 80,
                    channel: 0,
                },
            ],
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        },
        ClipState {
            id: ClipId(demo_id(2)),
            track_id: TrackId(demo_id(2)),
            start_tick: 0,
            duration_ticks: PPQN * 8,
            name: "Bass Line".into(),
            notes: vec![
                Note {
                    id: NoteId(200),
                    pitch: 36,
                    start_tick: 0,
                    duration_ticks: PPQN * 2,
                    velocity: 120,
                    channel: 0,
                },
                Note {
                    id: NoteId(201),
                    pitch: 40,
                    start_tick: PPQN * 2,
                    duration_ticks: PPQN * 2,
                    velocity: 110,
                    channel: 0,
                },
            ],
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        },
        ClipState {
            id: ClipId(demo_id(3)),
            track_id: TrackId(demo_id(3)),
            start_tick: 0,
            duration_ticks: PPQN * 16,
            name: "Drum Loop".into(),
            notes: Vec::new(),
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        },
        ClipState {
            id: ClipId(demo_id(4)),
            track_id: TrackId(demo_id(4)),
            start_tick: PPQN * 4,
            duration_ticks: PPQN * 12,
            name: "Pad Chords".into(),
            notes: vec![
                Note {
                    id: NoteId(300),
                    pitch: 60,
                    start_tick: PPQN * 4,
                    duration_ticks: PPQN * 4,
                    velocity: 70,
                    channel: 0,
                },
                Note {
                    id: NoteId(301),
                    pitch: 64,
                    start_tick: PPQN * 4,
                    duration_ticks: PPQN * 4,
                    velocity: 70,
                    channel: 0,
                },
                Note {
                    id: NoteId(302),
                    pitch: 67,
                    start_tick: PPQN * 4,
                    duration_ticks: PPQN * 4,
                    velocity: 70,
                    channel: 0,
                },
            ],
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        },
    ]
}
