//! Commands sent from the UI thread to the audio engine via SPSC ring buffer.
//!
//! **Thread safety:** All variants must be `Send`. No variant may contain types
//! that allocate on drop (the audio thread drains these commands and must not
//! trigger deallocation). For heap-allocated data (like audio clip data), use
//! `Arc<[f32]>` — the audio thread will hold a reference but never drop the last one.

use std::sync::Arc;

use crate::ids::{ClipId, TrackId};
use crate::midi_clip::MidiClip;
use crate::parameters::TrackConfig;
use crate::time::{SamplePos, Tick};

/// A command from the UI to the audio engine.
///
/// Commands are placed into an SPSC ring buffer (rtrb) by the UI thread
/// and drained at the beginning of each audio callback.
///
/// Design constraints:
/// - Must be `Send` (crosses thread boundary)
/// - Should be small (fits in ring buffer slot efficiently)
/// - No types that allocate on drop in audio-thread-critical variants
#[derive(Debug)]
pub enum EngineCommand {
    // ── Transport ──────────────────────────────────────────────
    /// Start playback from current position.
    Play,

    /// Stop playback and reset position to last play-start point.
    Stop,

    /// Pause playback at current position (resume with Play).
    Pause,

    /// Seek to an absolute sample position.
    SetPosition(SamplePos),

    /// Change the tempo (BPM).
    SetTempo(f64),

    /// Configure loop region.
    SetLoop {
        start: SamplePos,
        end: SamplePos,
        enabled: bool,
    },

    // ── Track control ──────────────────────────────────────────
    /// Set track volume (linear gain, 0.0–1.0+).
    SetTrackVolume { track_id: TrackId, volume: f32 },

    /// Set track pan (-1.0 = full left, 0.0 = center, 1.0 = full right).
    SetTrackPan { track_id: TrackId, pan: f32 },

    /// Mute or unmute a track.
    SetTrackMute { track_id: TrackId, mute: bool },

    /// Solo or unsolo a track.
    SetTrackSolo { track_id: TrackId, solo: bool },

    // ── Recording ──────────────────────────────────────────────
    /// Arm or disarm a track for recording.
    ArmTrack { track_id: TrackId, armed: bool },

    /// Set input monitoring mode for a track.
    SetMonitorMode {
        track_id: TrackId,
        mode: crate::parameters::MonitorMode,
    },

    /// Begin recording on all armed tracks (transport must be playing).
    StartRecording,

    /// Stop recording on all armed tracks.
    StopRecording,

    // ── Clip management (forwarded from topology processor) ────
    /// Install a MIDI clip into a track's MidiPlayerNode.
    /// The `Arc<MidiClip>` is safe on the audio thread because the UI always holds
    /// another reference — the audio thread never drops the last Arc.
    InstallMidiClip {
        track_id: TrackId,
        clip_id: ClipId,
        clip: Arc<crate::midi_clip::MidiClip>,
        start_tick: Tick,
    },

    /// Remove a MIDI clip from a track's MidiPlayerNode.
    RemoveMidiClipFromPlayer { track_id: TrackId, clip_id: ClipId },

    // ── Metronome ──────────────────────────────────────────────
    /// Enable or disable the metronome click.
    SetMetronomeEnabled(bool),

    // ── Engine lifecycle ───────────────────────────────────────
    /// Gracefully shut down the audio engine.
    Shutdown,
}

// Ensure EngineCommand is Send (required for cross-thread ring buffer).
// This is a compile-time check — if any variant contains !Send data, it fails.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<EngineCommand>();
};

/// Commands that involve heap allocation (String, Arc) and must NOT be processed
/// on the audio thread. These go through a separate crossbeam channel to the
/// graph-build thread.
#[derive(Debug)]
pub enum TopologyCommand {
    /// Add a new track to the audio graph.
    AddTrack {
        track_id: TrackId,
        config: TrackConfig,
    },

    /// Remove a track from the audio graph.
    RemoveTrack { track_id: TrackId },

    /// Load a pre-decoded audio clip for playback.
    /// `data` is an `Arc<[f32]>` (non-interleaved) so the audio thread only reads it
    /// and never drops the last reference (the project state on the UI side holds another Arc).
    LoadClip {
        track_id: TrackId,
        clip_id: ClipId,
        data: Arc<[f32]>,
        channels: usize,
        start_sample: SamplePos,
        length_samples: SamplePos,
    },

    /// Remove an audio clip from a track.
    RemoveClip { track_id: TrackId, clip_id: ClipId },

    /// Load a MIDI clip for playback on a track.
    /// `clip` is an `Arc<MidiClip>` — the audio thread only reads events
    /// and never drops the last reference.
    LoadMidiClip {
        track_id: TrackId,
        clip_id: ClipId,
        clip: Arc<MidiClip>,
        start_tick: Tick,
    },

    /// Remove a MIDI clip from a track.
    RemoveMidiClip { track_id: TrackId, clip_id: ClipId },
}

// Ensure TopologyCommand is Send.
const _TOPOLOGY_SEND: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<TopologyCommand>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ClipId, TrackId};

    // Compile-time Send assertions (duplicate the const checks as test-visible assertions).
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn engine_command_is_send() {
        assert_send::<EngineCommand>();
    }

    #[test]
    fn topology_command_is_send() {
        assert_send::<TopologyCommand>();
    }

    #[test]
    fn engine_command_is_sync() {
        // EngineCommand contains no interior mutability, so it should be Sync.
        assert_sync::<EngineCommand>();
    }

    #[test]
    fn topology_command_is_send_and_sync() {
        assert_send::<TopologyCommand>();
        assert_sync::<TopologyCommand>();
    }

    // Cover all EngineCommand variants — creation and pattern matching.
    #[test]
    fn engine_command_transport_variants() {
        let cmds: Vec<EngineCommand> = vec![
            EngineCommand::Play,
            EngineCommand::Stop,
            EngineCommand::Pause,
            EngineCommand::SetPosition(48000),
            EngineCommand::SetTempo(140.0),
            EngineCommand::SetLoop {
                start: 0,
                end: 96000,
                enabled: true,
            },
        ];
        assert_eq!(cmds.len(), 6);

        // Pattern matching coverage.
        for cmd in &cmds {
            match cmd {
                EngineCommand::Play => {}
                EngineCommand::Stop => {}
                EngineCommand::Pause => {}
                EngineCommand::SetPosition(pos) => assert_eq!(*pos, 48000),
                EngineCommand::SetTempo(bpm) => assert!((bpm - 140.0).abs() < f64::EPSILON),
                EngineCommand::SetLoop {
                    start,
                    end,
                    enabled,
                } => {
                    assert_eq!(*start, 0);
                    assert_eq!(*end, 96000);
                    assert!(*enabled);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn engine_command_track_control_variants() {
        let track_id = TrackId::new();
        let cmds = vec![
            EngineCommand::SetTrackVolume {
                track_id,
                volume: 0.75,
            },
            EngineCommand::SetTrackPan {
                track_id,
                pan: -0.5,
            },
            EngineCommand::SetTrackMute {
                track_id,
                mute: true,
            },
            EngineCommand::SetTrackSolo {
                track_id,
                solo: false,
            },
        ];
        assert_eq!(cmds.len(), 4);

        match &cmds[0] {
            EngineCommand::SetTrackVolume { volume, .. } => {
                assert!((volume - 0.75).abs() < f32::EPSILON);
            }
            _ => panic!("expected SetTrackVolume"),
        }
    }

    #[test]
    fn engine_command_recording_variants() {
        let track_id = TrackId::new();
        let cmds = vec![
            EngineCommand::ArmTrack {
                track_id,
                armed: true,
            },
            EngineCommand::StartRecording,
            EngineCommand::StopRecording,
        ];
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn engine_command_lifecycle_variants() {
        let cmd = EngineCommand::Shutdown;
        matches!(cmd, EngineCommand::Shutdown);
    }

    #[test]
    fn engine_command_debug_format() {
        let cmd = EngineCommand::Play;
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("Play"));
    }

    #[test]
    fn topology_command_all_variants() {
        use crate::midi_clip::MidiClip;

        let track_id = TrackId::new();
        let clip_id = ClipId::new();
        let data: Arc<[f32]> = Arc::from(vec![0.0f32; 1024].into_boxed_slice());
        let midi_clip = Arc::new(MidiClip::new(vec![], 960));

        let cmds: Vec<TopologyCommand> = vec![
            TopologyCommand::AddTrack {
                track_id,
                config: TrackConfig::default(),
            },
            TopologyCommand::RemoveTrack { track_id },
            TopologyCommand::LoadClip {
                track_id,
                clip_id,
                data,
                channels: 2,
                start_sample: 0,
                length_samples: 512,
            },
            TopologyCommand::RemoveClip { track_id, clip_id },
            TopologyCommand::LoadMidiClip {
                track_id,
                clip_id,
                clip: midi_clip,
                start_tick: 0,
            },
            TopologyCommand::RemoveMidiClip { track_id, clip_id },
        ];
        assert_eq!(cmds.len(), 6);

        // Pattern matching coverage.
        for cmd in &cmds {
            match cmd {
                TopologyCommand::AddTrack { config, .. } => {
                    assert_eq!(config.channel_count, 2);
                }
                TopologyCommand::RemoveTrack { track_id: id } => {
                    assert_eq!(*id, track_id);
                }
                TopologyCommand::LoadClip {
                    channels,
                    length_samples,
                    ..
                } => {
                    assert_eq!(*channels, 2);
                    assert_eq!(*length_samples, 512);
                }
                TopologyCommand::RemoveClip {
                    track_id: tid,
                    clip_id: cid,
                } => {
                    assert_eq!(*tid, track_id);
                    assert_eq!(*cid, clip_id);
                }
                TopologyCommand::LoadMidiClip {
                    start_tick, clip, ..
                } => {
                    assert_eq!(*start_tick, 0);
                    assert_eq!(clip.duration_ticks(), 960);
                }
                TopologyCommand::RemoveMidiClip {
                    track_id: tid,
                    clip_id: cid,
                } => {
                    assert_eq!(*tid, track_id);
                    assert_eq!(*cid, clip_id);
                }
            }
        }
    }

    #[test]
    fn topology_command_debug_format() {
        let cmd = TopologyCommand::RemoveTrack {
            track_id: TrackId::new(),
        };
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("RemoveTrack"));
    }
}
