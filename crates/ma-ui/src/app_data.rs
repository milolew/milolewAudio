//! AppData — root vizia Model for the DAW application.
//!
//! Owns all state and the engine bridge. Handles all events from views/widgets
//! and routes commands to the audio engine via lock-free ring buffers.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ma_core::undo::{UndoAction, UndoManager};

use crate::undo_actions;

use vizia::prelude::*;

use ma_audio_engine::device_manager::AudioDeviceManager;
use ma_audio_engine::engine::EngineConfig;
use ma_audio_engine::peak_cache::PeakCache;
use ma_core::commands::EngineCommand as CoreCommand;
use ma_core::device::AudioDeviceConfig;

use crate::config::{load_preferences, save_preferences, Preferences};
use crate::engine_bridge::bridge::{create_bridge, EngineBridge};
use crate::engine_bridge::commands::EngineCommand;
use crate::engine_bridge::mock_engine::{spawn_mock_engine, MockEngineHandle};
use crate::engine_bridge::real_bridge::RealEngineBridge;
use crate::engine_bridge::responses::EngineResponse;
use crate::state::arrangement_state::{ArrangementState, ClipSelection};
use crate::state::browser_state::{BrowserFilter, BrowserState};
use crate::state::mixer_state::MixerState;
use crate::state::piano_roll_state::{PianoRollState, PianoRollTool};
use crate::state::transport_state::TransportState;
use crate::types::midi::{Note, NoteId};
use crate::types::time::{QuantizeGrid, Tick, PPQN};
use crate::types::track::{ClipId, ClipState, TrackId, TrackKind, TrackState};
use crate::views::arrangement::clip_interaction::ClipInteraction;
use crate::views::arrangement::clipboard::ClipClipboard;
use crate::views::arrangement::snap::SnapGrid;

/// Maximum number of undo actions to keep in history.
const UNDO_MAX_DEPTH: usize = 100;

/// Which main view is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Data)]
pub enum ActiveView {
    Arrangement,
    Mixer,
    PianoRoll,
    Browser,
}

/// Bit depth for audio export.
#[derive(Debug, Clone, Copy)]
pub enum ExportBitDepth {
    Sixteen,
    ThirtyTwoFloat,
}

/// Root application data — the single vizia Model.
#[derive(Lens)]
pub struct AppData {
    pub transport: TransportState,
    pub arrangement: ArrangementState,
    pub mixer: MixerState,
    pub piano_roll: PianoRollState,
    pub tracks: Vec<TrackState>,
    pub clips: Vec<ClipState>,
    pub active_view: ActiveView,
    pub browser: BrowserState,
    pub device_status_text: String,
    pub device_sample_rate: String,
    pub device_buffer_size: String,
    pub device_latency: String,
    pub show_preferences: bool,

    #[lens(ignore)]
    engine: EngineMode,

    /// Pre-allocated buffer for engine responses — reused each frame to avoid allocation.
    #[lens(ignore)]
    response_buf: Vec<EngineResponse>,

    /// Peak caches for audio clips (for waveform rendering).
    #[lens(ignore)]
    pub audio_peaks: HashMap<ClipId, Arc<PeakCache>>,

    /// Audio data references — keeps Arc<[f32]> alive so the engine can read them.
    #[lens(ignore)]
    audio_data: HashMap<ClipId, Arc<[f32]>>,

    /// Undo/redo history manager.
    #[lens(ignore)]
    undo_manager: UndoManager<Self>,
}

/// Engine connection mode.
pub enum EngineMode {
    Real {
        device_manager: Box<AudioDeviceManager>,
        bridge: RealEngineBridge,
    },
    Mock {
        bridge: EngineBridge,
        _handle: MockEngineHandle,
    },
}

/// All events the UI can emit — consolidated from all view/widget actions.
pub enum AppEvent {
    /// Timer-driven poll of the engine ring buffer.
    PollEngine,
    /// Initialize the polling timer (called once on startup).
    InitTimer,

    // -- Preferences --
    ShowPreferences,
    HidePreferences,
    RefreshDevices,

    // -- Transport --
    Play,
    Stop,
    Record,
    Pause,
    TogglePlayPause,
    SetTempo(f64),
    SetPosition(Tick),
    ToggleLoop,
    ToggleMetronome,
    ToggleFollowPlayhead,
    SetLoopRegion {
        start: Tick,
        end: Tick,
    },

    // -- View switching --
    SwitchView(ActiveView),
    OpenPianoRoll(ClipId),

    // -- Track selection --
    SelectTrack(TrackId),

    // -- Mixer --
    SetTrackVolume {
        track_id: TrackId,
        volume: f32,
    },
    SetTrackPan {
        track_id: TrackId,
        pan: f32,
    },
    ToggleMute(TrackId),
    ToggleSolo(TrackId),

    // -- Arrangement scroll/zoom --
    ScrollArrangementX(f64),
    ScrollArrangementY(f32),
    ZoomArrangement(f64),

    // -- Piano roll editing --
    TransposeSelectedNotes {
        semitones: i8,
    },
    QuantizeSelectedNotes,
    SelectAllNotes,
    DeleteSelectedNotes,
    SetPianoRollTool(PianoRollTool),
    SetNoteVelocity {
        note_id: NoteId,
        velocity: u8,
    },
    FinishVelocityDrag {
        note_id: NoteId,
        original_velocity: u8,
        new_velocity: u8,
    },

    // -- Piano roll --
    AddNote(Note),
    RemoveNote(NoteId),
    MoveNote {
        note_id: NoteId,
        new_start: Tick,
        new_pitch: u8,
    },
    ResizeNote {
        note_id: NoteId,
        new_duration: Tick,
    },
    PreviewNoteOn {
        note: u8,
        velocity: u8,
    },
    PreviewNoteOff {
        note: u8,
    },
    UpdateInteraction(crate::state::piano_roll_state::PianoRollInteraction),
    SetQuantize(QuantizeGrid),

    // -- Piano roll scroll/zoom --
    ScrollPianoRollX(f64),
    ScrollPianoRollY(i8),
    ZoomPianoRoll(f64),

    // -- Project --
    SaveProject(PathBuf),
    LoadProject(PathBuf),
    ExportProject {
        path: PathBuf,
        sample_rate: u32,
        bit_depth: ExportBitDepth,
    },

    // -- Project shortcut --
    SaveCurrentProject,

    // -- Undo/Redo --
    Undo,
    Redo,

    // -- Browser --
    BrowserRefresh,
    BrowserGoUp,
    BrowserSelect(usize),
    BrowserActivate(usize),
    BrowserSetFilter(BrowserFilter),
    ToggleBrowser,

    // -- Recording --
    ToggleRecordArm(TrackId),

    // -- Arrangement clip operations --
    SelectClips(ClipSelection),
    UpdateClipInteraction(ClipInteraction),
    MoveClips {
        delta_tick: Tick,
        delta_track_index: i32,
    },
    ResizeClip {
        clip_id: ClipId,
        new_start: Tick,
        new_duration: Tick,
    },
    SplitClipAtPlayhead,
    DuplicateSelectedClips,
    DeleteSelectedClips,
    CopySelectedClips,
    PasteClips,
    SetSnapGrid(SnapGrid),

    // -- Track management --
    AddAudioTrack,
    AddMidiTrack,

    // -- Selection --
    SelectAllClips,
}

/// Create a deterministic UUID for demo data (stable across restarts).
fn demo_id(n: u64) -> uuid::Uuid {
    uuid::Uuid::from_u64_pair(0, n)
}

impl Default for AppData {
    fn default() -> Self {
        Self::new()
    }
}

impl AppData {
    /// Create AppData with demo tracks/clips and a mock engine.
    pub fn new() -> Self {
        let tracks = vec![
            TrackState::new_midi(TrackId(demo_id(1)), "Melody", [100, 160, 255]),
            TrackState::new_midi(TrackId(demo_id(2)), "Bass", [255, 140, 80]),
            TrackState::new_audio(TrackId(demo_id(3)), "Drums", [80, 220, 120]),
            TrackState::new_midi(TrackId(demo_id(4)), "Pad", [200, 100, 255]),
        ];

        let track_ids: Vec<TrackId> = tracks.iter().map(|t| t.id).collect();

        let clips = vec![
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
        ];

        // Load saved preferences (or defaults)
        let prefs = load_preferences();

        // Try real audio engine, fallback to mock
        let engine = Self::try_real_engine(&prefs.audio).unwrap_or_else(|e| {
            log::warn!("Real audio engine unavailable: {e}. Using mock engine.");
            let (bridge, endpoint) = create_bridge();
            let handle = spawn_mock_engine(endpoint, track_ids.clone());
            EngineMode::Mock {
                bridge,
                _handle: handle,
            }
        });

        let (device_status_text, device_sample_rate, device_buffer_size, device_latency) =
            match &engine {
                EngineMode::Real { device_manager, .. } => match device_manager.status() {
                    ma_core::device::DeviceStatus::Active {
                        output_device,
                        actual_sample_rate,
                        actual_buffer_size,
                        ..
                    } => {
                        let latency_ms =
                            *actual_buffer_size as f64 / *actual_sample_rate as f64 * 1000.0;
                        (
                            output_device.clone(),
                            format!("{actual_sample_rate} Hz"),
                            format!("{actual_buffer_size} samples"),
                            format!("{latency_ms:.1} ms"),
                        )
                    }
                    _ => ("Offline".into(), "-".into(), "-".into(), "-".into()),
                },
                EngineMode::Mock { .. } => (
                    "Mock Engine (no audio device)".into(),
                    format!("{} Hz", prefs.audio.sample_rate),
                    format!("{} samples", prefs.audio.buffer_size),
                    format!(
                        "{:.1} ms",
                        prefs.audio.buffer_size as f64 / prefs.audio.sample_rate as f64 * 1000.0
                    ),
                ),
            };

        Self {
            transport: TransportState::default(),
            arrangement: ArrangementState::default(),
            mixer: MixerState::default(),
            piano_roll: PianoRollState {
                next_note_id: 1000,
                ..Default::default()
            },
            tracks,
            clips,
            active_view: ActiveView::Arrangement,
            browser: BrowserState::default(),
            device_status_text,
            device_sample_rate,
            device_buffer_size,
            device_latency,
            show_preferences: false,
            engine,
            response_buf: Vec::with_capacity(64),
            audio_peaks: HashMap::new(),
            audio_data: HashMap::new(),
            undo_manager: UndoManager::new(UNDO_MAX_DEPTH),
        }
    }

    // -- Helper methods (ported from AppState) --

    pub fn track(&self, id: TrackId) -> Option<&TrackState> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&ClipState> {
        self.clips.iter().find(|c| c.id == id)
    }

    pub fn clips_for_track(&self, track_id: TrackId) -> Vec<&ClipState> {
        self.clips
            .iter()
            .filter(|c| c.track_id == track_id)
            .collect()
    }

    /// Whether there is an action available to undo.
    pub fn can_undo(&self) -> bool {
        self.undo_manager.can_undo()
    }

    /// Whether there is an action available to redo.
    pub fn can_redo(&self) -> bool {
        self.undo_manager.can_redo()
    }

    pub(crate) fn update_clip(&mut self, updated: ClipState) {
        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == updated.id) {
            *clip = updated;
        }
    }

    /// Attempt to start real audio engine with the given device config.
    fn try_real_engine(device_config: &AudioDeviceConfig) -> Result<EngineMode, String> {
        let mut device_manager = AudioDeviceManager::new();
        device_manager.enumerate_devices();
        let engine_config = EngineConfig::default();
        let handle = device_manager
            .apply_config(device_config.clone(), engine_config)
            .map_err(|e| e.to_string())?;
        let bridge = RealEngineBridge::new(handle);
        Ok(EngineMode::Real {
            device_manager: Box::new(device_manager),
            bridge,
        })
    }

    /// Send a UI command to whichever engine is active.
    pub(crate) fn send_command(&mut self, cmd: EngineCommand) {
        let sent = match &mut self.engine {
            EngineMode::Real { bridge, .. } => Self::translate_command(&cmd)
                .map(|core_cmd| bridge.send_command(core_cmd))
                .unwrap_or(true),
            EngineMode::Mock { bridge, .. } => bridge.send_command(cmd),
        };
        if !sent {
            log::error!("Engine command dropped — ring buffer full");
        }
    }

    /// Translate UI command to core engine command.
    fn translate_command(cmd: &EngineCommand) -> Option<CoreCommand> {
        match cmd {
            EngineCommand::Play => Some(CoreCommand::Play),
            EngineCommand::Stop => Some(CoreCommand::Stop),
            EngineCommand::Pause => Some(CoreCommand::Pause),
            EngineCommand::Record => Some(CoreCommand::StartRecording),
            EngineCommand::SetTempo(bpm) => Some(CoreCommand::SetTempo(*bpm)),
            EngineCommand::SetTrackVolume { track_id, volume } => {
                Some(CoreCommand::SetTrackVolume {
                    track_id: *track_id,
                    volume: *volume,
                })
            }
            EngineCommand::SetTrackPan { track_id, pan } => Some(CoreCommand::SetTrackPan {
                track_id: *track_id,
                pan: *pan,
            }),
            EngineCommand::SetTrackMute { track_id, mute } => Some(CoreCommand::SetTrackMute {
                track_id: *track_id,
                mute: *mute,
            }),
            EngineCommand::SetTrackSolo { track_id, solo } => Some(CoreCommand::SetTrackSolo {
                track_id: *track_id,
                solo: *solo,
            }),
            EngineCommand::ArmTrack { track_id, armed } => Some(CoreCommand::ArmTrack {
                track_id: *track_id,
                armed: *armed,
            }),
            EngineCommand::StopRecord => Some(CoreCommand::StopRecording),
            _ => None,
        }
    }

    // -- Audio file loading --

    /// Load decoded audio into the selected (or first Audio) track.
    fn load_audio_clip_into_track(
        &mut self,
        decoded: ma_audio_engine::audio_decode::DecodedAudio,
        name: &str,
        source_path: &std::path::Path,
    ) {
        // Find target track: selected track if Audio, else first Audio track
        let track_id = self
            .arrangement
            .selected_track
            .and_then(|id| {
                self.tracks
                    .iter()
                    .find(|t| t.id == id && t.kind == TrackKind::Audio)
            })
            .or_else(|| self.tracks.iter().find(|t| t.kind == TrackKind::Audio))
            .map(|t| t.id);

        let track_id = match track_id {
            Some(id) => id,
            None => {
                log::warn!("No audio track available — cannot load clip");
                return;
            }
        };

        let clip_id = ClipId::new();
        let data: Arc<[f32]> = Arc::from(decoded.samples.into_boxed_slice());

        // Build peak cache
        let peak_cache = ma_audio_engine::peak_cache::build_peak_cache(
            &data,
            decoded.channels,
            decoded.length_samples,
        );

        // Convert sample length to ticks
        let tempo = self.transport.tempo;
        let sample_rate = decoded.sample_rate as f64;
        let length_seconds = decoded.length_samples as f64 / sample_rate;
        let length_ticks = (length_seconds * tempo / 60.0 * PPQN as f64) as i64;

        let clip_state = ClipState {
            id: clip_id,
            track_id,
            start_tick: 0,
            duration_ticks: length_ticks.max(1),
            name: name.to_string(),
            notes: Vec::new(),
            audio_file: Some(source_path.to_string_lossy().to_string()),
            audio_length_samples: Some(decoded.length_samples),
            audio_sample_rate: Some(decoded.sample_rate),
        };

        self.clips.push(clip_state);
        self.audio_peaks.insert(clip_id, Arc::new(peak_cache));
        self.audio_data.insert(clip_id, Arc::clone(&data));

        // Send to engine (Real mode only)
        let start_sample = 0i64;
        if let EngineMode::Real { bridge, .. } = &self.engine {
            bridge.send_topology_command(ma_core::TopologyCommand::LoadClip {
                track_id,
                clip_id,
                data,
                channels: decoded.channels,
                start_sample,
                length_samples: decoded.length_samples as i64,
            });
        }

        log::info!(
            "Loaded audio clip '{name}' ({} samples, {}ch) into track {track_id:?}",
            decoded.length_samples,
            decoded.channels,
        );
    }

    // -- MIDI file loading --

    /// Load a parsed MIDI clip into the first available MIDI track.
    /// Creates a ClipState for the UI and sends InstallMidiClip to the engine.
    fn load_midi_clip_into_track(&mut self, clip: ma_core::midi_clip::MidiClip, name: &str) {
        use crate::types::track::TrackKind;

        // Find first MIDI track
        let midi_track = self.tracks.iter().find(|t| t.kind == TrackKind::Midi);
        let track_id = match midi_track {
            Some(t) => t.id,
            None => {
                log::warn!("No MIDI track available — cannot load clip");
                return;
            }
        };

        let clip_id = ClipId::new();
        let duration = clip.duration_ticks();
        let arc_clip = std::sync::Arc::new(clip);

        // Add clip to UI state
        let clip_state = ClipState {
            id: clip_id,
            track_id,
            start_tick: 0, // place at timeline start
            duration_ticks: duration,
            name: name.to_string(),
            notes: Vec::new(), // UI note display populated separately
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        };
        self.clips.push(clip_state);

        // Send to engine (Real only — Mock has no audio graph)
        if let EngineMode::Real { bridge, .. } = &mut self.engine {
            bridge.send_command(ma_core::EngineCommand::InstallMidiClip {
                track_id,
                clip_id,
                clip: arc_clip,
                start_tick: 0,
            });
        }

        log::info!("Loaded MIDI clip '{name}' into track {track_id:?}");
    }

    // -- Poll engine responses --

    fn poll_engine(&mut self) {
        // Take the pre-allocated buffer to avoid borrow conflicts between
        // self.engine and self.transport/self.mixer during response processing.
        let mut responses = std::mem::take(&mut self.response_buf);
        match &mut self.engine {
            EngineMode::Real { bridge, .. } => bridge.poll_responses(&mut responses),
            EngineMode::Mock { bridge, .. } => bridge.poll_responses(&mut responses),
        };
        for resp in &responses {
            match resp {
                EngineResponse::TransportUpdate {
                    position,
                    is_playing,
                    is_recording,
                } => {
                    self.transport.position = *position;
                    self.transport.is_playing = *is_playing;
                    self.transport.is_recording = *is_recording;
                }
                EngineResponse::TempoUpdate(bpm) => {
                    self.transport.tempo = *bpm;
                }
                EngineResponse::MeterUpdate {
                    track_id,
                    peak_l,
                    peak_r,
                } => {
                    self.mixer.update_meter(*track_id, *peak_l, *peak_r);
                }
                EngineResponse::MasterMeterUpdate { peak_l, peak_r } => {
                    self.mixer.master_peak_l = *peak_l;
                    self.mixer.master_peak_r = *peak_r;
                }
                EngineResponse::CpuLoad(load) => {
                    self.mixer.cpu_load = *load;
                }
                EngineResponse::RecordingComplete {
                    track_id,
                    path,
                    total_samples,
                } => {
                    self.handle_recording_complete(*track_id, path.clone(), *total_samples);
                }
                EngineResponse::RecordingError { track_id, error } => {
                    log::error!("Recording error on track {track_id:?}: {error}");
                }
            }
        }
        // Return the buffer for reuse next frame
        self.response_buf = responses;
    }

    fn handle_recording_complete(
        &mut self,
        track_id: TrackId,
        path: std::path::PathBuf,
        _total_samples: u64,
    ) {
        let record_start = self.transport.record_start_position.unwrap_or(0);

        // Decode the recorded WAV file
        let decoded = match crate::file_loader::load_audio_file(&path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to decode recording {}: {e}", path.display());
                return;
            }
        };

        let clip_id = ClipId::new();
        let data: Arc<[f32]> = Arc::from(decoded.samples.into_boxed_slice());

        // Build peak cache for waveform rendering
        let peak_cache = ma_audio_engine::peak_cache::build_peak_cache(
            &data,
            decoded.channels,
            decoded.length_samples,
        );

        // Compute duration in ticks from sample length
        let sample_rate = decoded.sample_rate as f64;
        let length_seconds = decoded.length_samples as f64 / sample_rate;
        let length_ticks = (length_seconds * self.transport.tempo / 60.0 * PPQN as f64) as i64;

        let clip_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Recording".into());

        let clip_state = ClipState {
            id: clip_id,
            track_id,
            start_tick: record_start,
            duration_ticks: length_ticks.max(1),
            name: clip_name.clone(),
            notes: Vec::new(),
            audio_file: Some(path.to_string_lossy().to_string()),
            audio_length_samples: Some(decoded.length_samples),
            audio_sample_rate: Some(decoded.sample_rate),
        };

        self.clips.push(clip_state);
        self.audio_peaks.insert(clip_id, Arc::new(peak_cache));
        self.audio_data.insert(clip_id, Arc::clone(&data));

        // Install in engine for playback
        if let EngineMode::Real { bridge, .. } = &self.engine {
            bridge.send_topology_command(ma_core::TopologyCommand::LoadClip {
                track_id,
                clip_id,
                data,
                channels: decoded.channels,
                start_sample: 0,
                length_samples: decoded.length_samples as i64,
            });
        }

        // Clear recording metadata for this track
        self.transport.recording_tracks.retain(|id| *id != track_id);
        if self.transport.recording_tracks.is_empty() {
            self.transport.record_start_position = None;
        }

        log::info!("Recording complete: clip '{clip_name}' on track {track_id:?}");
    }

    // -- Project save/load/export --

    fn save_project_to(&self, path: &Path) -> Result<(), String> {
        use ma_core::project_file::*;

        let project_dir = path.parent().unwrap_or(path);

        // Ensure audio directory exists
        let audio_dir = project_dir.join("audio");
        std::fs::create_dir_all(&audio_dir).map_err(|e| e.to_string())?;

        let tracks: Vec<TrackFile> = self
            .tracks
            .iter()
            .map(|track| {
                let clips: Vec<ClipFile> = self
                    .clips
                    .iter()
                    .filter(|c| c.track_id == track.id)
                    .map(|clip| {
                        // Copy audio file to project dir if needed
                        let relative_audio = clip.audio_file.as_ref().and_then(|src_path| {
                            let src = std::path::Path::new(src_path);
                            let filename = src.file_name()?;
                            let dest = audio_dir.join(filename);
                            if src.exists() && !dest.exists() {
                                let _ = std::fs::copy(src, &dest);
                            }
                            Some(format!("audio/{}", filename.to_string_lossy()))
                        });

                        ClipFile {
                            id: clip.id,
                            name: clip.name.clone(),
                            start_tick: clip.start_tick,
                            duration_ticks: clip.duration_ticks,
                            notes: clip
                                .notes
                                .iter()
                                .map(|n| NoteFile {
                                    pitch: n.pitch,
                                    start_tick: n.start_tick,
                                    duration_ticks: n.duration_ticks,
                                    velocity: n.velocity,
                                    channel: n.channel,
                                })
                                .collect(),
                            audio_file: relative_audio.or_else(|| clip.audio_file.clone()),
                            audio_length_samples: clip.audio_length_samples,
                            audio_sample_rate: clip.audio_sample_rate,
                        }
                    })
                    .collect();

                TrackFile {
                    id: track.id,
                    name: track.name.clone(),
                    kind: match track.kind {
                        TrackKind::Audio => TrackKindFile::Audio,
                        TrackKind::Midi => TrackKindFile::Midi,
                    },
                    color: track.color,
                    volume: track.volume,
                    pan: track.pan,
                    muted: track.mute,
                    clips,
                }
            })
            .collect();

        let sample_rate = match &self.engine {
            EngineMode::Real { bridge, .. } => bridge.sample_rate(),
            EngineMode::Mock { .. } => 48000,
        };

        let project = ProjectFile {
            version: PROJECT_VERSION,
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".into()),
            tempo: self.transport.tempo,
            sample_rate,
            tracks,
        };

        save_project(&project, path).map_err(|e| e.to_string())?;
        log::info!("Project saved to {}", path.display());
        Ok(())
    }

    fn load_project_from(&mut self, path: &Path) -> Result<(), String> {
        use ma_core::project_file::*;

        let project = load_project(path).map_err(|e| e.to_string())?;
        let project_dir = path.parent().unwrap_or(path);

        self.tracks.clear();
        self.clips.clear();
        self.audio_peaks.clear();
        self.audio_data.clear();
        self.undo_manager.clear();
        self.transport.tempo = project.tempo;

        let mut next_note_id = 1000u64;

        for track_file in &project.tracks {
            let kind = match track_file.kind {
                TrackKindFile::Audio => TrackKind::Audio,
                TrackKindFile::Midi => TrackKind::Midi,
            };

            self.tracks.push(TrackState {
                id: track_file.id,
                name: track_file.name.clone(),
                kind,
                volume: track_file.volume,
                pan: track_file.pan,
                mute: track_file.muted,
                solo: false,
                color: track_file.color,
                record_armed: false,
            });

            for clip_file in &track_file.clips {
                let notes: Vec<Note> = clip_file
                    .notes
                    .iter()
                    .map(|n| {
                        let id = NoteId(next_note_id);
                        next_note_id += 1;
                        Note {
                            id,
                            pitch: n.pitch,
                            start_tick: n.start_tick,
                            duration_ticks: n.duration_ticks,
                            velocity: n.velocity,
                            channel: n.channel,
                        }
                    })
                    .collect();

                // Resolve audio file path with traversal protection
                let audio_file_abs = clip_file.audio_file.as_ref().and_then(|rel| {
                    // Reject paths containing ".." to prevent directory traversal
                    if rel.contains("..") {
                        log::warn!("Audio file path contains '..', skipping: {rel}");
                        return None;
                    }
                    let resolved = project_dir.join(rel);
                    // If file exists, verify it stays within project directory
                    if let (Ok(canonical_dir), Ok(canonical_file)) =
                        (project_dir.canonicalize(), resolved.canonicalize())
                    {
                        if !canonical_file.starts_with(&canonical_dir) {
                            log::warn!("Audio file path escapes project directory: {rel}");
                            return None;
                        }
                    }
                    Some(resolved.to_string_lossy().to_string())
                });

                // Load audio data if present
                if let Some(ref abs_path) = audio_file_abs {
                    let audio_path = std::path::Path::new(abs_path);
                    if audio_path.exists() {
                        if let Ok(decoded) = crate::file_loader::load_audio_file(audio_path) {
                            let data: Arc<[f32]> = Arc::from(decoded.samples.into_boxed_slice());
                            let peak_cache = ma_audio_engine::peak_cache::build_peak_cache(
                                &data,
                                decoded.channels,
                                decoded.length_samples,
                            );
                            self.audio_peaks.insert(clip_file.id, Arc::new(peak_cache));
                            self.audio_data.insert(clip_file.id, Arc::clone(&data));

                            // Send to engine
                            if let EngineMode::Real { bridge, .. } = &self.engine {
                                bridge.send_topology_command(ma_core::TopologyCommand::LoadClip {
                                    track_id: track_file.id,
                                    clip_id: clip_file.id,
                                    data,
                                    channels: decoded.channels,
                                    start_sample: 0,
                                    length_samples: decoded.length_samples as i64,
                                });
                            }
                        }
                    }
                }

                self.clips.push(ClipState {
                    id: clip_file.id,
                    track_id: track_file.id,
                    start_tick: clip_file.start_tick,
                    duration_ticks: clip_file.duration_ticks,
                    name: clip_file.name.clone(),
                    notes,
                    audio_file: audio_file_abs,
                    audio_length_samples: clip_file.audio_length_samples,
                    audio_sample_rate: clip_file.audio_sample_rate,
                });
            }
        }

        self.piano_roll.next_note_id = next_note_id;
        log::info!("Project loaded from {}", path.display());
        Ok(())
    }

    fn export_project(&self, path: &Path, sample_rate: u32, bit_depth: ExportBitDepth) {
        use ma_audio_engine::export::*;

        let engine_config = EngineConfig {
            sample_rate,
            buffer_size: 256,
            initial_tracks: self
                .tracks
                .iter()
                .map(|t| {
                    (
                        t.id,
                        ma_core::TrackConfig {
                            name: t.name.clone(),
                            channel_count: 2,
                            input_enabled: false,
                            initial_volume: t.volume,
                            initial_pan: t.pan,
                            track_type: match t.kind {
                                TrackKind::Audio => ma_core::TrackType::Audio,
                                TrackKind::Midi => ma_core::TrackType::Midi,
                            },
                        },
                    )
                })
                .collect(),
        };

        let clips: Vec<ExportClip> = self
            .audio_data
            .iter()
            .filter_map(|(clip_id, data)| {
                let clip = self.clips.iter().find(|c| c.id == *clip_id)?;
                Some(ExportClip {
                    track_id: clip.track_id,
                    clip_id: *clip_id,
                    data: Arc::clone(data),
                    channels: 2,
                    start_sample: 0,
                    length_samples: clip.audio_length_samples.unwrap_or(0) as i64,
                })
            })
            .collect();

        // Collect MIDI clips for export
        let midi_clips: Vec<ExportMidiClip> = self
            .clips
            .iter()
            .filter(|c| !c.notes.is_empty())
            .map(|clip| {
                let events: Vec<ma_core::parameters::MidiEvent> = clip
                    .notes
                    .iter()
                    .flat_map(|n| {
                        vec![
                            ma_core::parameters::MidiEvent {
                                tick: n.start_tick,
                                message: ma_core::parameters::MidiMessage::NoteOn {
                                    channel: n.channel,
                                    note: n.pitch,
                                    velocity: n.velocity,
                                },
                            },
                            ma_core::parameters::MidiEvent {
                                tick: n.start_tick + n.duration_ticks,
                                message: ma_core::parameters::MidiMessage::NoteOff {
                                    channel: n.channel,
                                    note: n.pitch,
                                    velocity: 0,
                                },
                            },
                        ]
                    })
                    .collect();
                let duration = clip.duration_ticks;
                ExportMidiClip {
                    track_id: clip.track_id,
                    clip_id: clip.id,
                    clip: Arc::new(ma_core::MidiClip::new(events, duration)),
                    start_tick: clip.start_tick,
                }
            })
            .collect();

        // Find total duration in samples
        let max_tick = self
            .clips
            .iter()
            .map(|c| c.start_tick + c.duration_ticks)
            .max()
            .unwrap_or(0);
        let total_seconds = max_tick as f64 / PPQN as f64 * 60.0 / self.transport.tempo;
        let total_samples = (total_seconds * sample_rate as f64) as u64 + sample_rate as u64; // +1s padding

        let bd = match bit_depth {
            ExportBitDepth::Sixteen => BitDepth::Sixteen,
            ExportBitDepth::ThirtyTwoFloat => BitDepth::ThirtyTwoFloat,
        };

        let export_config = ExportConfig {
            sample_rate,
            bit_depth: bd,
        };

        let path = path.to_path_buf();
        log::info!("Export started: {}", path.display());
        // Run export on background thread to avoid blocking UI
        // NOTE: JoinHandle is dropped — no UI feedback on completion yet (known limitation)
        std::thread::spawn(move || {
            match offline_render(
                engine_config,
                &clips,
                &midi_clips,
                total_samples,
                &path,
                &export_config,
            ) {
                Ok(()) => log::info!("Export complete: {}", path.display()),
                Err(e) => log::error!("Export failed: {e}"),
            }
        });
    }

    // -- Dispatch helpers --

    fn dispatch_transport(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Play => {
                self.send_command(EngineCommand::Play);
            }
            AppEvent::Stop => {
                self.stop_recording_if_active();
                self.send_command(EngineCommand::Stop);
            }
            AppEvent::Record => {
                self.start_recording();
            }
            AppEvent::Pause => {
                self.send_command(EngineCommand::Pause);
            }
            AppEvent::SetTempo(bpm) => {
                self.send_command(EngineCommand::SetTempo(*bpm));
            }
            AppEvent::SetPosition(tick) => {
                self.send_command(EngineCommand::SetPosition(*tick));
            }
            AppEvent::TogglePlayPause => {
                if self.transport.is_playing {
                    self.send_command(EngineCommand::Pause);
                } else {
                    self.send_command(EngineCommand::Play);
                }
            }
            AppEvent::ToggleLoop => {
                self.transport.loop_enabled = !self.transport.loop_enabled;
            }
            AppEvent::ToggleMetronome => {
                self.transport.metronome_enabled = !self.transport.metronome_enabled;
            }
            AppEvent::ToggleFollowPlayhead => {
                self.transport.follow_playhead = !self.transport.follow_playhead;
            }
            AppEvent::SetLoopRegion { start, end } => {
                self.transport.loop_start = *start;
                self.transport.loop_end = *end;
                self.transport.loop_enabled = true;
            }
            _ => {}
        }
    }

    fn start_recording(&mut self) {
        let armed_tracks: Vec<_> = self
            .tracks
            .iter()
            .filter(|t| t.record_armed)
            .map(|t| (t.id, t.name.clone()))
            .collect();

        if armed_tracks.is_empty() {
            log::warn!("Record pressed but no tracks are armed");
            return;
        }

        self.transport.record_start_position = Some(self.transport.position);
        self.transport.recording_tracks = armed_tracks.iter().map(|(id, _)| *id).collect();

        // Send StartRecording to engine (sets is_recording on armed track nodes)
        self.send_command(EngineCommand::Record);

        // Start disk recording for each armed track
        if let EngineMode::Real { bridge, .. } = &mut self.engine {
            let sample_rate = bridge.sample_rate();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let recording_dir = std::path::PathBuf::from("recordings");
            if let Err(e) = std::fs::create_dir_all(&recording_dir) {
                log::error!("Failed to create recordings directory: {e}");
                return;
            }

            for (track_id, track_name) in &armed_tracks {
                let safe_name = track_name.replace(' ', "_");
                let path = recording_dir.join(format!("{safe_name}_{timestamp}.wav"));
                bridge.start_recording_for_track(*track_id, path, 2, sample_rate);
            }
        }
    }

    fn stop_recording_if_active(&mut self) {
        if !self.transport.is_recording {
            return;
        }

        let recording_tracks = std::mem::take(&mut self.transport.recording_tracks);

        if let EngineMode::Real { bridge, .. } = &self.engine {
            for track_id in &recording_tracks {
                bridge.stop_recording_for_track(*track_id);
            }
        }

        // StopRecording command is sent BEFORE Stop in engine
        self.send_command(EngineCommand::StopRecord);
    }

    fn dispatch_piano_roll(&mut self, event: &AppEvent) {
        let clip_id = match self.piano_roll.active_clip_id {
            Some(id) => id,
            None => return,
        };

        match event {
            AppEvent::AddNote(note) => {
                let note = Note {
                    id: self.piano_roll.alloc_note_id(),
                    ..*note
                };
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    let new_clip = clip.with_note_added(note);
                    self.update_clip(new_clip);
                    self.send_command(EngineCommand::AddNote { clip_id, note });
                    self.undo_manager
                        .push(Box::new(undo_actions::AddNoteAction { clip_id, note }));
                }
            }
            AppEvent::RemoveNote(note_id) => {
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    if let Some(note) = clip.notes.iter().find(|n| n.id == *note_id) {
                        let saved_note = *note;
                        let new_clip = clip.with_note_removed(*note_id);
                        self.update_clip(new_clip);
                        self.send_command(EngineCommand::RemoveNote {
                            clip_id,
                            note_id: *note_id,
                        });
                        self.undo_manager
                            .push(Box::new(undo_actions::RemoveNoteAction {
                                clip_id,
                                note: saved_note,
                            }));
                    }
                }
            }
            AppEvent::MoveNote {
                note_id,
                new_start,
                new_pitch,
            } => {
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    if let Some(note) = clip.notes.iter().find(|n| n.id == *note_id) {
                        let old_start = note.start_tick;
                        let old_pitch = note.pitch;
                        let updated = Note {
                            start_tick: *new_start,
                            pitch: *new_pitch,
                            ..*note
                        };
                        let new_clip = clip.with_note_updated(updated);
                        self.update_clip(new_clip);
                        self.send_command(EngineCommand::MoveNote {
                            clip_id,
                            note_id: *note_id,
                            new_start: *new_start,
                            new_pitch: *new_pitch,
                        });
                        self.undo_manager
                            .push(Box::new(undo_actions::MoveNoteAction {
                                clip_id,
                                note_id: *note_id,
                                old_start,
                                old_pitch,
                                new_start: *new_start,
                                new_pitch: *new_pitch,
                            }));
                    }
                }
            }
            AppEvent::ResizeNote {
                note_id,
                new_duration,
            } => {
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    if let Some(note) = clip.notes.iter().find(|n| n.id == *note_id) {
                        let old_duration = note.duration_ticks;
                        let updated = Note {
                            duration_ticks: *new_duration,
                            ..*note
                        };
                        let new_clip = clip.with_note_updated(updated);
                        self.update_clip(new_clip);
                        self.send_command(EngineCommand::ResizeNote {
                            clip_id,
                            note_id: *note_id,
                            new_duration: *new_duration,
                        });
                        self.undo_manager
                            .push(Box::new(undo_actions::ResizeNoteAction {
                                clip_id,
                                note_id: *note_id,
                                old_duration,
                                new_duration: *new_duration,
                            }));
                    }
                }
            }
            AppEvent::PreviewNoteOn { note, velocity } => {
                self.send_command(EngineCommand::NoteOn {
                    channel: 0,
                    note: *note,
                    velocity: *velocity,
                });
            }
            AppEvent::PreviewNoteOff { note } => {
                self.send_command(EngineCommand::NoteOff {
                    channel: 0,
                    note: *note,
                    velocity: 0,
                });
            }
            AppEvent::UpdateInteraction(interaction) => {
                self.piano_roll.interaction = interaction.clone();
            }
            AppEvent::SetQuantize(grid) => {
                self.piano_roll.quantize = *grid;
            }
            _ => {}
        }
    }

    fn handle_transpose_selected(&mut self, semitones: i8) {
        let clip_id = match self.piano_roll.active_clip_id {
            Some(id) => id,
            None => return,
        };
        let selected = &self.piano_roll.selected_notes;
        if selected.is_empty() {
            return;
        }
        let clip = match self.clips.iter().find(|c| c.id == clip_id) {
            Some(c) => c,
            None => return,
        };
        let original_pitches: Vec<(NoteId, u8)> = clip
            .notes
            .iter()
            .filter(|n| selected.contains(&n.id))
            .map(|n| (n.id, n.pitch))
            .collect();
        if original_pitches.is_empty() {
            return;
        }
        let action = undo_actions::TransposeNotesAction {
            clip_id,
            original_pitches,
            semitones,
        };
        action.apply(self);
        self.undo_manager.push(Box::new(action));
    }

    fn handle_quantize_selected(&mut self) {
        let clip_id = match self.piano_roll.active_clip_id {
            Some(id) => id,
            None => return,
        };
        let grid = self.piano_roll.quantize;
        if grid == QuantizeGrid::Off {
            return;
        }
        let selected = &self.piano_roll.selected_notes;
        if selected.is_empty() {
            return;
        }
        let clip = match self.clips.iter().find(|c| c.id == clip_id) {
            Some(c) => c,
            None => return,
        };
        let original_starts: Vec<(NoteId, Tick)> = clip
            .notes
            .iter()
            .filter(|n| selected.contains(&n.id))
            .map(|n| (n.id, n.start_tick))
            .collect();
        if original_starts.is_empty() {
            return;
        }
        let action = undo_actions::QuantizeNotesAction {
            clip_id,
            original_starts,
            quantize_grid: grid,
        };
        action.apply(self);
        self.undo_manager.push(Box::new(action));
    }

    fn handle_delete_selected_notes(&mut self) {
        let clip_id = match self.piano_roll.active_clip_id {
            Some(id) => id,
            None => return,
        };
        let selected: Vec<NoteId> = self.piano_roll.selected_notes.drain(..).collect();
        if selected.is_empty() {
            return;
        }
        let clip = match self.clips.iter().find(|c| c.id == clip_id) {
            Some(c) => c,
            None => return,
        };
        let notes: Vec<Note> = clip
            .notes
            .iter()
            .filter(|n| selected.contains(&n.id))
            .copied()
            .collect();
        if notes.is_empty() {
            return;
        }
        let action = undo_actions::RemoveNotesAction { clip_id, notes };
        action.apply(self);
        self.undo_manager.push(Box::new(action));
    }

    fn handle_add_track(&mut self, kind: TrackKind) {
        let track_id = TrackId(uuid::Uuid::new_v4());
        let (name, color) = match kind {
            TrackKind::Audio => ("Audio", [80, 220, 120]),
            TrackKind::Midi => ("MIDI", [100, 160, 255]),
        };
        let track = match kind {
            TrackKind::Audio => TrackState::new_audio(track_id, name, color),
            TrackKind::Midi => TrackState::new_midi(track_id, name, color),
        };
        let action = undo_actions::AddTrackAction {
            track: track.clone(),
        };
        action.apply(self);
        self.undo_manager.push(Box::new(action));
    }

    fn handle_set_note_velocity(&mut self, note_id: NoteId, velocity: u8) {
        let clip_id = match self.piano_roll.active_clip_id {
            Some(id) => id,
            None => return,
        };
        if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
            let mut updated_clip = clip.clone();
            if let Some(note) = updated_clip.notes.iter_mut().find(|n| n.id == note_id) {
                note.velocity = velocity;
            }
            self.update_clip(updated_clip);
        }
    }
}

impl Model for AppData {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _meta| match app_event {
            // Timer: poll engine ring buffer
            AppEvent::PollEngine => {
                self.poll_engine();
            }

            // Initialize polling timer (called once on startup from root_view)
            AppEvent::InitTimer => {
                let timer = cx.add_timer(Duration::from_millis(16), None, |cx, action| {
                    if let TimerAction::Tick(_) = action {
                        cx.emit(AppEvent::PollEngine);
                    }
                });
                cx.start_timer(timer);
            }

            // Transport
            AppEvent::Play
            | AppEvent::Stop
            | AppEvent::Record
            | AppEvent::Pause
            | AppEvent::TogglePlayPause
            | AppEvent::SetTempo(_)
            | AppEvent::SetPosition(_)
            | AppEvent::ToggleLoop
            | AppEvent::ToggleMetronome
            | AppEvent::ToggleFollowPlayhead
            | AppEvent::SetLoopRegion { .. } => {
                self.dispatch_transport(app_event);
            }

            // Preferences
            AppEvent::ShowPreferences => {
                self.show_preferences = true;
            }
            AppEvent::HidePreferences => {
                self.show_preferences = false;
            }
            AppEvent::RefreshDevices => {
                if let EngineMode::Real { device_manager, .. } = &mut self.engine {
                    device_manager.enumerate_devices();
                    match device_manager.status() {
                        ma_core::device::DeviceStatus::Active {
                            output_device,
                            actual_sample_rate,
                            actual_buffer_size,
                            ..
                        } => {
                            let latency_ms =
                                *actual_buffer_size as f64 / *actual_sample_rate as f64 * 1000.0;
                            self.device_status_text = output_device.clone();
                            self.device_sample_rate = format!("{actual_sample_rate} Hz");
                            self.device_buffer_size = format!("{actual_buffer_size} samples");
                            self.device_latency = format!("{latency_ms:.1} ms");

                            // Persist current device config
                            let prefs = Preferences {
                                audio: AudioDeviceConfig {
                                    output_device_name: Some(output_device.clone()),
                                    sample_rate: *actual_sample_rate,
                                    buffer_size: *actual_buffer_size,
                                    ..AudioDeviceConfig::default()
                                },
                            };
                            save_preferences(&prefs);
                        }
                        _ => {
                            self.device_status_text = "Offline".into();
                            self.device_sample_rate = "-".into();
                            self.device_buffer_size = "-".into();
                            self.device_latency = "-".into();
                        }
                    }
                }
            }

            // View switching
            AppEvent::SwitchView(view) => {
                self.active_view = *view;
            }
            AppEvent::OpenPianoRoll(clip_id) => {
                self.piano_roll.active_clip_id = Some(*clip_id);
                self.active_view = ActiveView::PianoRoll;
            }

            // Track selection
            AppEvent::SelectTrack(id) => {
                self.arrangement.selected_track = Some(*id);
            }

            // Mixer: track parameters
            AppEvent::SetTrackVolume { track_id, volume } => {
                let old_volume =
                    if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                        let old = track.volume;
                        track.volume = *volume;
                        Some(old)
                    } else {
                        None
                    };
                self.send_command(EngineCommand::SetTrackVolume {
                    track_id: *track_id,
                    volume: *volume,
                });
                if let Some(old_vol) = old_volume {
                    self.undo_manager
                        .push(Box::new(undo_actions::SetTrackVolumeAction {
                            track_id: *track_id,
                            old_volume: old_vol,
                            new_volume: *volume,
                        }));
                }
            }
            AppEvent::SetTrackPan { track_id, pan } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                    track.pan = *pan;
                }
                self.send_command(EngineCommand::SetTrackPan {
                    track_id: *track_id,
                    pan: *pan,
                });
            }
            AppEvent::ToggleMute(track_id) => {
                let new_mute =
                    if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                        track.mute = !track.mute;
                        Some(track.mute)
                    } else {
                        None
                    };
                if let Some(mute) = new_mute {
                    self.send_command(EngineCommand::SetTrackMute {
                        track_id: *track_id,
                        mute,
                    });
                }
            }
            AppEvent::ToggleSolo(track_id) => {
                let new_solo =
                    if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                        track.solo = !track.solo;
                        Some(track.solo)
                    } else {
                        None
                    };
                if let Some(solo) = new_solo {
                    self.send_command(EngineCommand::SetTrackSolo {
                        track_id: *track_id,
                        solo,
                    });
                }
            }

            // Recording: arm toggle
            AppEvent::ToggleRecordArm(track_id) => {
                let new_armed =
                    if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                        track.record_armed = !track.record_armed;
                        Some(track.record_armed)
                    } else {
                        None
                    };
                if let Some(armed) = new_armed {
                    self.send_command(EngineCommand::ArmTrack {
                        track_id: *track_id,
                        armed,
                    });
                }
            }

            // Arrangement scroll/zoom
            AppEvent::ScrollArrangementX(dx) => {
                self.arrangement.scroll_x = (self.arrangement.scroll_x + dx).max(0.0);
            }
            AppEvent::ScrollArrangementY(dy) => {
                self.arrangement.scroll_y += dy;
            }
            AppEvent::ZoomArrangement(factor) => {
                self.arrangement.zoom_x = (self.arrangement.zoom_x * factor).clamp(0.001, 1.0);
            }

            // Piano roll note editing
            AppEvent::AddNote(_)
            | AppEvent::RemoveNote(_)
            | AppEvent::MoveNote { .. }
            | AppEvent::ResizeNote { .. }
            | AppEvent::PreviewNoteOn { .. }
            | AppEvent::PreviewNoteOff { .. }
            | AppEvent::UpdateInteraction(_)
            | AppEvent::SetQuantize(_) => {
                self.dispatch_piano_roll(app_event);
            }

            // Piano roll scroll/zoom
            AppEvent::ScrollPianoRollX(dx) => {
                self.piano_roll.scroll_x = (self.piano_roll.scroll_x + dx).max(0.0);
            }
            AppEvent::ScrollPianoRollY(dy) => {
                self.piano_roll.scroll_y =
                    (self.piano_roll.scroll_y as i16 + *dy as i16).clamp(0, 127) as u8;
            }
            AppEvent::ZoomPianoRoll(factor) => {
                self.piano_roll.zoom_x = (self.piano_roll.zoom_x * factor).clamp(0.01, 2.0);
            }

            // Project save/load/export
            AppEvent::SaveProject(path) => {
                if let Err(e) = self.save_project_to(path) {
                    log::error!("Failed to save project: {e}");
                }
            }
            AppEvent::LoadProject(path) => {
                if let Err(e) = self.load_project_from(path) {
                    log::error!("Failed to load project: {e}");
                }
            }
            AppEvent::ExportProject {
                path,
                sample_rate,
                bit_depth,
            } => {
                self.export_project(path, *sample_rate, *bit_depth);
            }

            // Browser
            AppEvent::BrowserRefresh => {
                self.browser.refresh();
            }
            AppEvent::BrowserGoUp => {
                self.browser.go_up();
            }
            AppEvent::BrowserSelect(index) => {
                self.browser.selected_index = Some(*index);
            }
            AppEvent::BrowserActivate(index) => {
                if let Some(entry) = self.browser.entries.get(*index).cloned() {
                    if entry.is_dir {
                        self.browser.enter_dir(entry.path);
                    } else if entry.is_midi() {
                        match crate::file_loader::load_midi_file(&entry.path) {
                            Ok(clip) => self.load_midi_clip_into_track(clip, &entry.name),
                            Err(e) => log::warn!("Failed to load MIDI file: {e}"),
                        }
                    } else if entry.is_audio() {
                        match crate::file_loader::load_audio_file(&entry.path) {
                            Ok(decoded) => {
                                self.load_audio_clip_into_track(decoded, &entry.name, &entry.path);
                            }
                            Err(e) => log::warn!("Failed to load audio file: {e}"),
                        }
                    }
                }
            }
            AppEvent::BrowserSetFilter(filter) => {
                self.browser.filter = *filter;
                self.browser.refresh();
            }
            // -- Undo/Redo --
            AppEvent::Undo => {
                let mut um =
                    std::mem::replace(&mut self.undo_manager, UndoManager::new(UNDO_MAX_DEPTH));
                um.undo(self);
                self.undo_manager = um;
            }
            AppEvent::Redo => {
                let mut um =
                    std::mem::replace(&mut self.undo_manager, UndoManager::new(UNDO_MAX_DEPTH));
                um.redo(self);
                self.undo_manager = um;
            }

            AppEvent::ToggleBrowser => {
                self.browser.visible = !self.browser.visible;
                if self.browser.visible {
                    self.active_view = ActiveView::Browser;
                    self.browser.refresh();
                } else {
                    self.active_view = ActiveView::Arrangement;
                }
            }

            // Arrangement clip operations
            AppEvent::SelectClips(_)
            | AppEvent::UpdateClipInteraction(_)
            | AppEvent::MoveClips { .. }
            | AppEvent::ResizeClip { .. }
            | AppEvent::SplitClipAtPlayhead
            | AppEvent::DuplicateSelectedClips
            | AppEvent::DeleteSelectedClips
            | AppEvent::CopySelectedClips
            | AppEvent::PasteClips
            | AppEvent::SetSnapGrid(_) => {
                self.dispatch_clip_operations(app_event);
            }

            // -- Piano roll editing shortcuts --
            AppEvent::TransposeSelectedNotes { semitones } => {
                self.handle_transpose_selected(*semitones);
            }
            AppEvent::QuantizeSelectedNotes => {
                self.handle_quantize_selected();
            }
            AppEvent::SelectAllNotes => {
                if let Some(clip_id) = self.piano_roll.active_clip_id {
                    if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                        self.piano_roll.selected_notes = clip.notes.iter().map(|n| n.id).collect();
                    }
                }
            }
            AppEvent::DeleteSelectedNotes => {
                self.handle_delete_selected_notes();
            }
            AppEvent::SetPianoRollTool(tool) => {
                self.piano_roll.tool = *tool;
            }
            AppEvent::SetNoteVelocity { note_id, velocity } => {
                self.handle_set_note_velocity(*note_id, *velocity);
            }
            AppEvent::FinishVelocityDrag {
                note_id,
                original_velocity,
                new_velocity,
            } => {
                if original_velocity != new_velocity {
                    let clip_id = match self.piano_roll.active_clip_id {
                        Some(id) => id,
                        None => return,
                    };
                    self.undo_manager
                        .push(Box::new(undo_actions::SetNoteVelocityAction {
                            clip_id,
                            note_id: *note_id,
                            old_velocity: *original_velocity,
                            new_velocity: *new_velocity,
                        }));
                }
            }

            // -- Project shortcut --
            AppEvent::SaveCurrentProject => {
                let path = PathBuf::from("project.mla");
                if let Err(e) = self.save_project_to(&path) {
                    log::error!("Failed to save project: {e}");
                }
            }

            // -- Track management --
            AppEvent::AddAudioTrack => {
                self.handle_add_track(TrackKind::Audio);
            }
            AppEvent::AddMidiTrack => {
                self.handle_add_track(TrackKind::Midi);
            }

            // -- Selection --
            AppEvent::SelectAllClips => {
                let all_ids: HashSet<ClipId> = self.clips.iter().map(|c| c.id).collect();
                self.arrangement.selected_clips = ClipSelection { clips: all_ids };
            }
        });
    }
}

impl AppData {
    /// Dispatch arrangement clip operation events.
    fn dispatch_clip_operations(&mut self, event: &AppEvent) {
        match event {
            AppEvent::SelectClips(selection) => {
                self.arrangement.selected_clips = selection.clone();
            }
            AppEvent::UpdateClipInteraction(interaction) => {
                self.arrangement.interaction = interaction.clone();
            }
            AppEvent::SetSnapGrid(grid) => {
                self.arrangement.snap_grid = *grid;
            }
            AppEvent::MoveClips {
                delta_tick,
                delta_track_index,
            } => {
                if self.tracks.is_empty() {
                    return;
                }
                let selected: Vec<ClipId> = self
                    .arrangement
                    .selected_clips
                    .clips
                    .iter()
                    .copied()
                    .collect();
                for clip_id in selected {
                    if let Some(idx) = self.clips.iter().position(|c| c.id == clip_id) {
                        let clip = &self.clips[idx];
                        let new_start = (clip.start_tick + delta_tick).max(0);

                        // Compute target track from delta index
                        let current_track_idx = self
                            .tracks
                            .iter()
                            .position(|t| t.id == clip.track_id)
                            .unwrap_or(0) as i32;
                        let target_idx = (current_track_idx + delta_track_index)
                            .clamp(0, self.tracks.len() as i32 - 1)
                            as usize;
                        let new_track_id = self.tracks[target_idx].id;

                        // Remove from engine at old position
                        self.remove_clip_from_engine(clip_id, clip.track_id);

                        // Update clip state (immutably)
                        let new_clip = ClipState {
                            start_tick: new_start,
                            track_id: new_track_id,
                            ..self.clips[idx].clone()
                        };
                        self.clips[idx] = new_clip;

                        // Reinstall in engine at new position
                        self.install_clip_in_engine(clip_id);
                    }
                }
            }
            AppEvent::ResizeClip {
                clip_id,
                new_start,
                new_duration,
            } => {
                if let Some(idx) = self.clips.iter().position(|c| c.id == *clip_id) {
                    let old_track_id = self.clips[idx].track_id;
                    self.remove_clip_from_engine(*clip_id, old_track_id);

                    let old = &self.clips[idx];

                    // For MIDI clips: trim notes outside new range (immutable)
                    let trimmed_notes = if old.audio_file.is_none() {
                        let clip_end = *new_duration;
                        old.notes
                            .iter()
                            .filter(|n| n.start_tick < clip_end)
                            .map(|n| {
                                let dur = if n.start_tick + n.duration_ticks > clip_end {
                                    clip_end - n.start_tick
                                } else {
                                    n.duration_ticks
                                };
                                Note {
                                    duration_ticks: dur,
                                    ..*n
                                }
                            })
                            .collect()
                    } else {
                        old.notes.clone()
                    };

                    let new_clip = ClipState {
                        start_tick: *new_start,
                        duration_ticks: *new_duration,
                        notes: trimmed_notes,
                        ..old.clone()
                    };

                    self.clips[idx] = new_clip;
                    self.install_clip_in_engine(*clip_id);
                }
            }
            AppEvent::SplitClipAtPlayhead => {
                let playhead = self.transport.position;
                let selected: Vec<ClipId> = self
                    .arrangement
                    .selected_clips
                    .clips
                    .iter()
                    .copied()
                    .collect();
                let mut new_right_ids = Vec::new();

                for clip_id in selected {
                    let Some(idx) = self.clips.iter().position(|c| c.id == clip_id) else {
                        continue;
                    };
                    let clip = self.clips[idx].clone();
                    let clip_end = clip.start_tick + clip.duration_ticks;

                    // Only split if playhead is strictly inside the clip
                    if playhead <= clip.start_tick || playhead >= clip_end {
                        continue;
                    }

                    self.remove_clip_from_engine(clip_id, clip.track_id);

                    let left_duration = playhead - clip.start_tick;

                    // Left half: keep original ID, shorten duration (immutable)
                    let left_notes = if clip.audio_file.is_none() {
                        clip.notes
                            .iter()
                            .filter(|n| n.start_tick < left_duration)
                            .map(|n| {
                                let dur = if n.start_tick + n.duration_ticks > left_duration {
                                    left_duration - n.start_tick
                                } else {
                                    n.duration_ticks
                                };
                                Note {
                                    duration_ticks: dur,
                                    ..*n
                                }
                            })
                            .collect()
                    } else {
                        clip.notes.clone()
                    };
                    let left = ClipState {
                        duration_ticks: left_duration,
                        notes: left_notes,
                        ..clip.clone()
                    };

                    // Right half: new ID, starts at playhead (immutable)
                    let right_id = ClipId::new();
                    let right_notes = if clip.audio_file.is_none() {
                        clip.notes
                            .iter()
                            .filter_map(|n| {
                                let note_end = n.start_tick + n.duration_ticks;
                                if note_end <= left_duration {
                                    return None; // note entirely in left half
                                }
                                let (new_start, new_dur) = if n.start_tick < left_duration {
                                    let overshoot = left_duration - n.start_tick;
                                    (0, n.duration_ticks.saturating_sub(overshoot))
                                } else {
                                    (n.start_tick - left_duration, n.duration_ticks)
                                };
                                if new_dur == 0 {
                                    return None; // filter zero-duration notes
                                }
                                Some(Note {
                                    start_tick: new_start,
                                    duration_ticks: new_dur,
                                    ..*n
                                })
                            })
                            .collect()
                    } else {
                        clip.notes.clone()
                    };
                    let right = ClipState {
                        id: right_id,
                        start_tick: playhead,
                        duration_ticks: clip_end - playhead,
                        name: format!("{} (R)", clip.name),
                        notes: right_notes,
                        ..clip.clone()
                    };

                    self.clips[idx] = left;
                    self.install_clip_in_engine(clip_id);

                    new_right_ids.push(right_id);
                    self.clips.push(right);
                    self.install_clip_in_engine(right_id);
                }

                // Select both halves
                if !new_right_ids.is_empty() {
                    let mut sel = self.arrangement.selected_clips.clips.clone();
                    for id in new_right_ids {
                        sel.insert(id);
                    }
                    self.arrangement.selected_clips = ClipSelection { clips: sel };
                }
            }
            AppEvent::DuplicateSelectedClips => {
                let selected: Vec<ClipId> = self
                    .arrangement
                    .selected_clips
                    .clips
                    .iter()
                    .copied()
                    .collect();
                let mut new_ids = Vec::new();

                for clip_id in selected {
                    if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id).cloned() {
                        let new_id = ClipId::new();
                        let mut dup = clip.clone();
                        dup.id = new_id;
                        dup.start_tick = clip.start_tick + clip.duration_ticks;
                        dup.name = format!("{} (copy)", clip.name);

                        // New note IDs for MIDI clips
                        for note in &mut dup.notes {
                            note.id = self.piano_roll.alloc_note_id();
                        }

                        // Share audio data
                        if let Some(peaks) = self.audio_peaks.get(&clip.id).cloned() {
                            self.audio_peaks.insert(new_id, peaks);
                        }
                        if let Some(data) = self.audio_data.get(&clip.id).cloned() {
                            self.audio_data.insert(new_id, data);
                        }

                        new_ids.push(new_id);
                        self.clips.push(dup);
                        self.install_clip_in_engine(new_id);
                    }
                }

                // Select the duplicates
                self.arrangement.selected_clips = ClipSelection {
                    clips: new_ids.into_iter().collect(),
                };
            }
            AppEvent::DeleteSelectedClips => {
                let selected: Vec<ClipId> = self
                    .arrangement
                    .selected_clips
                    .clips
                    .iter()
                    .copied()
                    .collect();

                for clip_id in &selected {
                    if let Some(clip) = self.clips.iter().find(|c| c.id == *clip_id).cloned() {
                        self.remove_clip_from_engine(*clip_id, clip.track_id);
                        self.audio_peaks.remove(clip_id);
                        self.audio_data.remove(clip_id);
                    }
                }

                self.clips.retain(|c| !selected.contains(&c.id));
                self.arrangement.selected_clips = ClipSelection::default();
            }
            AppEvent::CopySelectedClips => {
                let selected_clips: Vec<ClipState> = self
                    .clips
                    .iter()
                    .filter(|c| self.arrangement.selected_clips.contains(&c.id))
                    .cloned()
                    .collect();
                let track_map: Vec<(TrackId, usize)> = self
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (t.id, i))
                    .collect();
                self.arrangement.clipboard = ClipClipboard::from_clips(&selected_clips, &track_map);
            }
            AppEvent::PasteClips => {
                if self.arrangement.clipboard.is_empty() || self.tracks.is_empty() {
                    return;
                }
                let playhead = self.transport.position;
                let entries = self.arrangement.clipboard.entries.clone();
                let mut new_ids = Vec::new();

                // Determine base track index from selected track
                let base_track_idx = self
                    .arrangement
                    .selected_track
                    .and_then(|tid| self.tracks.iter().position(|t| t.id == tid))
                    .unwrap_or(0) as i32;

                for entry in &entries {
                    let target_idx = (base_track_idx + entry.track_index_offset)
                        .clamp(0, self.tracks.len() as i32 - 1)
                        as usize;
                    let target_track_id = self.tracks[target_idx].id;

                    let new_id = ClipId::new();
                    let mut new_clip = entry.clip.clone();
                    new_clip.id = new_id;
                    new_clip.track_id = target_track_id;
                    new_clip.start_tick = playhead + entry.tick_offset;

                    // New note IDs for MIDI clips
                    for note in &mut new_clip.notes {
                        note.id = self.piano_roll.alloc_note_id();
                    }

                    // Share audio data
                    if let Some(peaks) = self.audio_peaks.get(&entry.clip.id).cloned() {
                        self.audio_peaks.insert(new_id, peaks);
                    }
                    if let Some(data) = self.audio_data.get(&entry.clip.id).cloned() {
                        self.audio_data.insert(new_id, data);
                    }

                    new_ids.push(new_id);
                    self.clips.push(new_clip);
                    self.install_clip_in_engine(new_id);
                }

                self.arrangement.selected_clips = ClipSelection {
                    clips: new_ids.into_iter().collect(),
                };
            }
            _ => {}
        }
    }

    /// Remove a clip from the audio engine.
    ///
    /// Sends the appropriate removal command based on clip type:
    /// - MIDI clips: `RemoveMidiClipFromPlayer` via RT ring buffer (consumed by command_processor)
    /// - Audio clips: `RemoveClip` via topology channel
    fn remove_clip_from_engine(&mut self, clip_id: ClipId, track_id: TrackId) {
        let is_audio = self
            .clips
            .iter()
            .find(|c| c.id == clip_id)
            .map(|c| c.audio_file.is_some())
            .unwrap_or(false);

        if let EngineMode::Real { bridge, .. } = &mut self.engine {
            if is_audio {
                bridge.send_topology_command(ma_core::TopologyCommand::RemoveClip {
                    track_id,
                    clip_id,
                });
            } else {
                // Use RT command for MIDI — topology channel has no consumer yet
                bridge.send_command(ma_core::EngineCommand::RemoveMidiClipFromPlayer {
                    track_id,
                    clip_id,
                });
            }
        }
    }

    /// (Re)install a clip in the audio engine based on its current state.
    fn install_clip_in_engine(&mut self, clip_id: ClipId) {
        let clip = match self.clips.iter().find(|c| c.id == clip_id) {
            Some(c) => c.clone(),
            None => return,
        };

        // Prepare engine commands before borrowing engine mutably
        if clip.audio_file.is_some() {
            let data = self.audio_data.get(&clip_id).map(Arc::clone);
            if let Some(data) = data {
                let length_samples = clip.audio_length_samples.unwrap_or(0) as i64;
                // Infer channel count from data length / sample count
                let total_samples = data.len();
                let channels = if length_samples > 0 {
                    (total_samples as i64 / length_samples).max(1) as usize
                } else {
                    2
                };
                if let EngineMode::Real { bridge, .. } = &self.engine {
                    bridge.send_topology_command(ma_core::TopologyCommand::LoadClip {
                        track_id: clip.track_id,
                        clip_id: clip.id,
                        data,
                        channels,
                        start_sample: 0,
                        length_samples,
                    });
                }
            }
        } else if clip.audio_file.is_none() {
            // MIDI clip — install even if notes are empty (to clear stale engine state)
            use ma_core::parameters::{MidiEvent, MidiMessage};
            let events: Vec<MidiEvent> = clip
                .notes
                .iter()
                .flat_map(|n| {
                    [
                        MidiEvent {
                            tick: n.start_tick,
                            message: MidiMessage::NoteOn {
                                channel: n.channel,
                                note: n.pitch,
                                velocity: n.velocity,
                            },
                        },
                        MidiEvent {
                            tick: n.start_tick + n.duration_ticks,
                            message: MidiMessage::NoteOff {
                                channel: n.channel,
                                note: n.pitch,
                                velocity: 0,
                            },
                        },
                    ]
                })
                .collect();
            let midi_clip = std::sync::Arc::new(ma_core::midi_clip::MidiClip::new(
                events,
                clip.duration_ticks,
            ));
            if let EngineMode::Real { bridge, .. } = &mut self.engine {
                bridge.send_command(ma_core::EngineCommand::InstallMidiClip {
                    track_id: clip.track_id,
                    clip_id: clip.id,
                    clip: midi_clip,
                    start_tick: clip.start_tick,
                });
            }
        }
    }
}
