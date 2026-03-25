//! File loader — parse audio and MIDI files for import into the project.
//!
//! Currently supports MIDI files (.mid, .midi) via the `midly` crate.
//! Audio file loading (WAV, FLAC, etc.) requires an async decode pipeline
//! and is planned for a future iteration.

use std::path::Path;

use ma_core::midi_clip::MidiClip;
use ma_core::parameters::{MidiEvent, MidiMessage};

/// Errors that can occur during file loading.
#[derive(Debug)]
pub enum FileLoadError {
    /// File could not be read from disk.
    Io(std::io::Error),
    /// MIDI file could not be parsed.
    Parse(String),
    /// File contains no usable events.
    Empty,
}

impl std::fmt::Display for FileLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::Empty => write!(f, "file contains no MIDI events"),
        }
    }
}

impl From<std::io::Error> for FileLoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Parse a Standard MIDI File and return a `MidiClip`.
///
/// Converts delta ticks to absolute ticks and extracts NoteOn/NoteOff events.
/// All other event types (CC, pitch bend, sysex, meta) are ignored for now.
///
/// The returned clip's duration is the tick of the last event, rounded up
/// to the next whole beat (using the file's ticks-per-beat).
pub fn load_midi_file(path: &Path) -> Result<MidiClip, FileLoadError> {
    let bytes = std::fs::read(path)?;
    let smf = midly::Smf::parse(&bytes).map_err(|e| FileLoadError::Parse(e.to_string()))?;

    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(tpb) => tpb.as_int() as i64,
        midly::Timing::Timecode(_, _) => {
            // SMPTE timing — use 480 as a reasonable default
            480
        }
    };

    let mut events = Vec::new();
    let mut max_tick: i64 = 0;

    for track in &smf.tracks {
        let mut abs_tick: i64 = 0;

        for event in track {
            abs_tick += event.delta.as_int() as i64;

            if let midly::TrackEventKind::Midi { channel, message } = event.kind {
                let ch = channel.as_int();
                match message {
                    midly::MidiMessage::NoteOn { key, vel } => {
                        // velocity 0 = NoteOff per MIDI spec
                        let msg = if vel.as_int() == 0 {
                            MidiMessage::NoteOff {
                                channel: ch,
                                note: key.as_int(),
                                velocity: 0,
                            }
                        } else {
                            MidiMessage::NoteOn {
                                channel: ch,
                                note: key.as_int(),
                                velocity: vel.as_int(),
                            }
                        };
                        events.push(MidiEvent {
                            tick: abs_tick,
                            message: msg,
                        });
                    }
                    midly::MidiMessage::NoteOff { key, vel } => {
                        events.push(MidiEvent {
                            tick: abs_tick,
                            message: MidiMessage::NoteOff {
                                channel: ch,
                                note: key.as_int(),
                                velocity: vel.as_int(),
                            },
                        });
                    }
                    _ => {} // CC, pitch bend, etc. — ignored for now
                }
            }

            if abs_tick > max_tick {
                max_tick = abs_tick;
            }
        }
    }

    if events.is_empty() {
        return Err(FileLoadError::Empty);
    }

    // Round duration up to next beat boundary
    let duration_ticks = ((max_tick / ticks_per_beat) + 1) * ticks_per_beat;

    Ok(MidiClip::new(events, duration_ticks))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal SMF (format 0, 1 track, 480 tpb) with the given track events.
    fn build_smf_bytes(track_events: &[(u32, midly::TrackEventKind<'_>)]) -> Vec<u8> {
        use midly::*;

        let events: Vec<TrackEvent<'_>> = track_events
            .iter()
            .map(|(delta, kind)| TrackEvent {
                delta: (*delta).into(),
                kind: kind.clone(),
            })
            .collect();

        let smf = Smf {
            header: Header {
                format: Format::SingleTrack,
                timing: Timing::Metrical(480.into()),
            },
            tracks: vec![events],
        };

        let mut buf = Vec::new();
        smf.write_std(&mut buf).unwrap();
        buf
    }

    #[test]
    fn parse_note_on_off() {
        let bytes = build_smf_bytes(&[
            (
                0,
                midly::TrackEventKind::Midi {
                    channel: 0.into(),
                    message: midly::MidiMessage::NoteOn {
                        key: 60.into(),
                        vel: 100.into(),
                    },
                },
            ),
            (
                480,
                midly::TrackEventKind::Midi {
                    channel: 0.into(),
                    message: midly::MidiMessage::NoteOff {
                        key: 60.into(),
                        vel: 0.into(),
                    },
                },
            ),
            (
                0,
                midly::TrackEventKind::Meta(midly::MetaMessage::EndOfTrack),
            ),
        ]);

        let tmp = std::env::temp_dir().join("test_parse_note_on_off.mid");
        std::fs::write(&tmp, &bytes).unwrap();

        let clip = load_midi_file(&tmp).unwrap();
        assert_eq!(clip.event_count(), 2);

        let events = clip.events();
        assert_eq!(events[0].tick, 0);
        assert!(matches!(
            events[0].message,
            MidiMessage::NoteOn {
                note: 60,
                velocity: 100,
                ..
            }
        ));
        assert_eq!(events[1].tick, 480);
        assert!(matches!(
            events[1].message,
            MidiMessage::NoteOff { note: 60, .. }
        ));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn velocity_zero_is_note_off() {
        let bytes = build_smf_bytes(&[
            (
                0,
                midly::TrackEventKind::Midi {
                    channel: 0.into(),
                    message: midly::MidiMessage::NoteOn {
                        key: 64.into(),
                        vel: 80.into(),
                    },
                },
            ),
            (
                240,
                midly::TrackEventKind::Midi {
                    channel: 0.into(),
                    message: midly::MidiMessage::NoteOn {
                        key: 64.into(),
                        vel: 0.into(), // velocity 0 = NoteOff
                    },
                },
            ),
            (
                0,
                midly::TrackEventKind::Meta(midly::MetaMessage::EndOfTrack),
            ),
        ]);

        let tmp = std::env::temp_dir().join("test_vel_zero.mid");
        std::fs::write(&tmp, &bytes).unwrap();

        let clip = load_midi_file(&tmp).unwrap();
        let events = clip.events();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[1].message, MidiMessage::NoteOff { .. }));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn empty_file_returns_error() {
        let bytes = build_smf_bytes(&[(
            0,
            midly::TrackEventKind::Meta(midly::MetaMessage::EndOfTrack),
        )]);

        let tmp = std::env::temp_dir().join("test_empty_midi.mid");
        std::fs::write(&tmp, &bytes).unwrap();

        let result = load_midi_file(&tmp);
        assert!(matches!(result, Err(FileLoadError::Empty)));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn nonexistent_file_returns_io_error() {
        let result = load_midi_file(Path::new("/nonexistent/path/test.mid"));
        assert!(matches!(result, Err(FileLoadError::Io(_))));
    }

    #[test]
    fn duration_rounds_to_beat_boundary() {
        // NoteOn at tick 0, NoteOff at tick 100 (less than one beat at 480 tpb)
        let bytes = build_smf_bytes(&[
            (
                0,
                midly::TrackEventKind::Midi {
                    channel: 0.into(),
                    message: midly::MidiMessage::NoteOn {
                        key: 72.into(),
                        vel: 90.into(),
                    },
                },
            ),
            (
                100,
                midly::TrackEventKind::Midi {
                    channel: 0.into(),
                    message: midly::MidiMessage::NoteOff {
                        key: 72.into(),
                        vel: 0.into(),
                    },
                },
            ),
            (
                0,
                midly::TrackEventKind::Meta(midly::MetaMessage::EndOfTrack),
            ),
        ]);

        let tmp = std::env::temp_dir().join("test_duration_round.mid");
        std::fs::write(&tmp, &bytes).unwrap();

        let clip = load_midi_file(&tmp).unwrap();
        // max_tick=100, 100/480=0, (0+1)*480 = 480
        assert_eq!(clip.duration_ticks(), 480);

        std::fs::remove_file(&tmp).ok();
    }
}
