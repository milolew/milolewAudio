//! Project file serialization — save/load DAW project state to/from JSON.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ids::{ClipId, TrackId};
use crate::time::Tick;

/// Current project file format version.
pub const PROJECT_VERSION: u32 = 1;

/// Top-level project file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub name: String,
    pub tempo: f64,
    pub sample_rate: u32,
    pub tracks: Vec<TrackFile>,
}

/// Track kind discriminator for project files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKindFile {
    Audio,
    Midi,
}

/// A track in the project file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackFile {
    pub id: TrackId,
    pub name: String,
    pub kind: TrackKindFile,
    pub color: [u8; 3],
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
    pub clips: Vec<ClipFile>,
}

/// A clip in the project file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipFile {
    pub id: ClipId,
    pub name: String,
    pub start_tick: Tick,
    pub duration_ticks: Tick,
    pub notes: Vec<NoteFile>,
    /// Relative path to audio file (within project directory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_length_samples: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_sample_rate: Option<u32>,
}

/// A MIDI note in the project file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoteFile {
    pub pitch: u8,
    pub start_tick: Tick,
    pub duration_ticks: Tick,
    pub velocity: u8,
    pub channel: u8,
}

/// Errors that can occur during project file operations.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("version mismatch: file version {found} is newer than supported {expected}")]
    VersionMismatch { expected: u32, found: u32 },
}

/// Save a project to a JSON file.
pub fn save_project(project: &ProjectFile, path: &Path) -> Result<(), ProjectError> {
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(writer, project)?;
    Ok(())
}

/// Load a project from a JSON file.
///
/// Accepts files with version <= `PROJECT_VERSION` (forward compatible).
/// Rejects files with version > `PROJECT_VERSION` (from a newer app version).
pub fn load_project(path: &Path) -> Result<ProjectFile, ProjectError> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let project: ProjectFile = serde_json::from_reader(reader)?;

    if project.version > PROJECT_VERSION {
        return Err(ProjectError::VersionMismatch {
            expected: PROJECT_VERSION,
            found: project.version,
        });
    }

    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ClipId, TrackId};

    fn test_project() -> ProjectFile {
        ProjectFile {
            version: PROJECT_VERSION,
            name: "Test Project".into(),
            tempo: 120.0,
            sample_rate: 48000,
            tracks: vec![
                TrackFile {
                    id: TrackId::new(),
                    name: "MIDI Track".into(),
                    kind: TrackKindFile::Midi,
                    color: [100, 160, 255],
                    volume: 0.8,
                    pan: 0.0,
                    muted: false,
                    clips: vec![ClipFile {
                        id: ClipId::new(),
                        name: "Melody".into(),
                        start_tick: 0,
                        duration_ticks: 7680,
                        notes: vec![
                            NoteFile {
                                pitch: 60,
                                start_tick: 0,
                                duration_ticks: 480,
                                velocity: 100,
                                channel: 0,
                            },
                            NoteFile {
                                pitch: 64,
                                start_tick: 480,
                                duration_ticks: 480,
                                velocity: 90,
                                channel: 0,
                            },
                        ],
                        audio_file: None,
                        audio_length_samples: None,
                        audio_sample_rate: None,
                    }],
                },
                TrackFile {
                    id: TrackId::new(),
                    name: "Audio Track".into(),
                    kind: TrackKindFile::Audio,
                    color: [80, 220, 120],
                    volume: 1.0,
                    pan: -0.3,
                    muted: false,
                    clips: vec![ClipFile {
                        id: ClipId::new(),
                        name: "Drums".into(),
                        start_tick: 0,
                        duration_ticks: 15360,
                        notes: vec![],
                        audio_file: Some("audio/drums.wav".into()),
                        audio_length_samples: Some(48000),
                        audio_sample_rate: Some(48000),
                    }],
                },
            ],
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let project = test_project();
        let path = std::env::temp_dir().join("test_project_roundtrip.json");

        save_project(&project, &path).unwrap();
        let loaded = load_project(&path).unwrap();

        assert_eq!(loaded.version, project.version);
        assert_eq!(loaded.name, project.name);
        assert!((loaded.tempo - project.tempo).abs() < f64::EPSILON);
        assert_eq!(loaded.sample_rate, project.sample_rate);
        assert_eq!(loaded.tracks.len(), 2);

        let midi_track = &loaded.tracks[0];
        assert_eq!(midi_track.kind, TrackKindFile::Midi);
        assert_eq!(midi_track.clips.len(), 1);
        assert_eq!(midi_track.clips[0].notes.len(), 2);
        assert_eq!(midi_track.clips[0].notes[0].pitch, 60);

        let audio_track = &loaded.tracks[1];
        assert_eq!(audio_track.kind, TrackKindFile::Audio);
        assert_eq!(
            audio_track.clips[0].audio_file,
            Some("audio/drums.wav".into())
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn version_from_future_rejected() {
        let mut project = test_project();
        project.version = 999;

        let path = std::env::temp_dir().join("test_project_version.json");
        save_project(&project, &path).unwrap();

        let result = load_project(&path);
        assert!(matches!(
            result,
            Err(ProjectError::VersionMismatch {
                expected: 1,
                found: 999
            })
        ));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn version_from_past_accepted() {
        // Version 0 (older than current 1) should load fine
        let mut project = test_project();
        project.version = 0;

        let path = std::env::temp_dir().join("test_project_version_old.json");
        save_project(&project, &path).unwrap();

        let loaded = load_project(&path).unwrap();
        assert_eq!(loaded.version, 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_nonexistent_file() {
        let result = load_project(Path::new("/nonexistent/project.json"));
        assert!(matches!(result, Err(ProjectError::Io(_))));
    }

    #[test]
    fn audio_clip_optional_fields_omitted_when_none() {
        let project = ProjectFile {
            version: PROJECT_VERSION,
            name: "Minimal".into(),
            tempo: 120.0,
            sample_rate: 48000,
            tracks: vec![TrackFile {
                id: TrackId::new(),
                name: "T1".into(),
                kind: TrackKindFile::Midi,
                color: [100, 100, 100],
                volume: 1.0,
                pan: 0.0,
                muted: false,
                clips: vec![ClipFile {
                    id: ClipId::new(),
                    name: "C1".into(),
                    start_tick: 0,
                    duration_ticks: 960,
                    notes: vec![],
                    audio_file: None,
                    audio_length_samples: None,
                    audio_sample_rate: None,
                }],
            }],
        };

        let json = serde_json::to_string_pretty(&project).unwrap();
        // Optional fields should not appear in JSON when None
        assert!(!json.contains("audio_file"));
        assert!(!json.contains("audio_length_samples"));
        assert!(!json.contains("audio_sample_rate"));
    }
}
