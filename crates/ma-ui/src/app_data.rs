//! AppData — root vizia Model for the DAW application.
//!
//! Owns all state and the engine bridge. Handles all events from views/widgets
//! and routes commands to the audio engine via lock-free ring buffers.

use std::time::Duration;

use vizia::prelude::*;

use ma_audio_engine::device_manager::AudioDeviceManager;
use ma_audio_engine::engine::EngineConfig;
use ma_core::commands::EngineCommand as CoreCommand;
use ma_core::device::AudioDeviceConfig;

use crate::engine_bridge::bridge::{create_bridge, EngineBridge};
use crate::engine_bridge::commands::EngineCommand;
use crate::engine_bridge::mock_engine::{spawn_mock_engine, MockEngineHandle};
use crate::engine_bridge::real_bridge::RealEngineBridge;
use crate::engine_bridge::responses::EngineResponse;
use crate::state::arrangement_state::ArrangementState;
use crate::state::mixer_state::MixerState;
use crate::state::piano_roll_state::PianoRollState;
use crate::state::transport_state::TransportState;
use crate::types::midi::{Note, NoteId};
use crate::types::time::{QuantizeGrid, Tick, PPQN};
use crate::types::track::{ClipId, ClipState, TrackId, TrackState};

/// Which main view is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Data)]
pub enum ActiveView {
    Arrangement,
    Mixer,
    PianoRoll,
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
    pub device_status_text: String,
    pub show_preferences: bool,

    #[lens(ignore)]
    engine: EngineMode,
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
    SetTempo(f64),
    SetPosition(Tick),
    ToggleLoop,

    // -- View switching --
    SwitchView(ActiveView),
    OpenPianoRoll(ClipId),

    // -- Track selection --
    SelectTrack(TrackId),

    // -- Mixer --
    SetTrackVolume { track_id: TrackId, volume: f32 },
    SetTrackPan { track_id: TrackId, pan: f32 },
    ToggleMute(TrackId),
    ToggleSolo(TrackId),

    // -- Arrangement scroll/zoom --
    ScrollArrangementX(f64),
    ScrollArrangementY(f32),
    ZoomArrangement(f64),

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
    PreviewNoteOn { note: u8, velocity: u8 },
    PreviewNoteOff { note: u8 },
    UpdateInteraction(crate::state::piano_roll_state::PianoRollInteraction),
    SetQuantize(QuantizeGrid),
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
                    Note { id: NoteId(100), pitch: 60, start_tick: 0, duration_ticks: PPQN / 2, velocity: 100, channel: 0 },
                    Note { id: NoteId(101), pitch: 64, start_tick: PPQN / 2, duration_ticks: PPQN / 2, velocity: 90, channel: 0 },
                    Note { id: NoteId(102), pitch: 67, start_tick: PPQN, duration_ticks: PPQN, velocity: 110, channel: 0 },
                    Note { id: NoteId(103), pitch: 72, start_tick: PPQN * 2, duration_ticks: PPQN * 2, velocity: 80, channel: 0 },
                ],
            },
            ClipState {
                id: ClipId(demo_id(2)),
                track_id: TrackId(demo_id(2)),
                start_tick: 0,
                duration_ticks: PPQN * 8,
                name: "Bass Line".into(),
                notes: vec![
                    Note { id: NoteId(200), pitch: 36, start_tick: 0, duration_ticks: PPQN * 2, velocity: 120, channel: 0 },
                    Note { id: NoteId(201), pitch: 40, start_tick: PPQN * 2, duration_ticks: PPQN * 2, velocity: 110, channel: 0 },
                ],
            },
            ClipState {
                id: ClipId(demo_id(3)),
                track_id: TrackId(demo_id(3)),
                start_tick: 0,
                duration_ticks: PPQN * 16,
                name: "Drum Loop".into(),
                notes: Vec::new(),
            },
            ClipState {
                id: ClipId(demo_id(4)),
                track_id: TrackId(demo_id(4)),
                start_tick: PPQN * 4,
                duration_ticks: PPQN * 12,
                name: "Pad Chords".into(),
                notes: vec![
                    Note { id: NoteId(300), pitch: 60, start_tick: PPQN * 4, duration_ticks: PPQN * 4, velocity: 70, channel: 0 },
                    Note { id: NoteId(301), pitch: 64, start_tick: PPQN * 4, duration_ticks: PPQN * 4, velocity: 70, channel: 0 },
                    Note { id: NoteId(302), pitch: 67, start_tick: PPQN * 4, duration_ticks: PPQN * 4, velocity: 70, channel: 0 },
                ],
            },
        ];

        // Try real audio engine, fallback to mock
        let engine = Self::try_real_engine().unwrap_or_else(|e| {
            log::warn!("Real audio engine unavailable: {e}. Using mock engine.");
            let (bridge, endpoint) = create_bridge();
            let handle = spawn_mock_engine(endpoint, track_ids.clone());
            EngineMode::Mock { bridge, _handle: handle }
        });

        let device_status_text = match &engine {
            EngineMode::Real { device_manager, .. } => {
                match device_manager.status() {
                    ma_core::device::DeviceStatus::Active { output_device, actual_sample_rate, actual_buffer_size, .. } =>
                        format!("{output_device} ({actual_sample_rate} Hz, {actual_buffer_size} buf)"),
                    _ => "Offline".into(),
                }
            }
            EngineMode::Mock { .. } => "Mock Engine (no audio device)".into(),
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
            device_status_text,
            show_preferences: false,
            engine,
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
        self.clips.iter().filter(|c| c.track_id == track_id).collect()
    }

    fn update_clip(&mut self, updated: ClipState) {
        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == updated.id) {
            *clip = updated;
        }
    }

    /// Attempt to start real audio engine with default device.
    fn try_real_engine() -> Result<EngineMode, String> {
        let mut device_manager = AudioDeviceManager::new();
        device_manager.enumerate_devices();
        let device_config = AudioDeviceConfig::default();
        let engine_config = EngineConfig::default();
        let handle = device_manager
            .apply_config(device_config, engine_config)
            .map_err(|e| e.to_string())?;
        let bridge = RealEngineBridge::new(handle);
        Ok(EngineMode::Real { device_manager: Box::new(device_manager), bridge })
    }

    /// Send a UI command to whichever engine is active.
    fn send_command(&mut self, cmd: EngineCommand) {
        match &mut self.engine {
            EngineMode::Real { bridge, .. } => {
                if let Some(core_cmd) = Self::translate_command(&cmd) {
                    bridge.send_command(core_cmd);
                }
            }
            EngineMode::Mock { bridge, .. } => {
                bridge.send_command(cmd);
            }
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
            EngineCommand::SetTrackVolume { track_id, volume } =>
                Some(CoreCommand::SetTrackVolume { track_id: *track_id, volume: *volume }),
            EngineCommand::SetTrackPan { track_id, pan } =>
                Some(CoreCommand::SetTrackPan { track_id: *track_id, pan: *pan }),
            EngineCommand::SetTrackMute { track_id, mute } =>
                Some(CoreCommand::SetTrackMute { track_id: *track_id, mute: *mute }),
            EngineCommand::SetTrackSolo { track_id, solo } =>
                Some(CoreCommand::SetTrackSolo { track_id: *track_id, solo: *solo }),
            _ => None,
        }
    }

    // -- Poll engine responses --

    fn poll_engine(&mut self) {
        let responses = match &mut self.engine {
            EngineMode::Real { bridge, .. } => bridge.poll_responses(),
            EngineMode::Mock { bridge, .. } => bridge.poll_responses(),
        };
        for resp in responses {
            match resp {
                EngineResponse::TransportUpdate { position, is_playing, is_recording } => {
                    self.transport.position = position;
                    self.transport.is_playing = is_playing;
                    self.transport.is_recording = is_recording;
                }
                EngineResponse::TempoUpdate(bpm) => {
                    self.transport.tempo = bpm;
                }
                EngineResponse::MeterUpdate { track_id, peak_l, peak_r } => {
                    self.mixer.update_meter(track_id, peak_l, peak_r);
                }
                EngineResponse::CpuLoad(load) => {
                    self.mixer.cpu_load = load;
                }
            }
        }
    }

    // -- Dispatch helpers --

    fn dispatch_transport(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Play => { self.send_command(EngineCommand::Play); }
            AppEvent::Stop => { self.send_command(EngineCommand::Stop); }
            AppEvent::Record => { self.send_command(EngineCommand::Record); }
            AppEvent::Pause => { self.send_command(EngineCommand::Pause); }
            AppEvent::SetTempo(bpm) => { self.send_command(EngineCommand::SetTempo(*bpm)); }
            AppEvent::SetPosition(tick) => { self.send_command(EngineCommand::SetPosition(*tick)); }
            AppEvent::ToggleLoop => { self.transport.loop_enabled = !self.transport.loop_enabled; }
            _ => {}
        }
    }

    fn dispatch_piano_roll(&mut self, event: &AppEvent) {
        let clip_id = match self.piano_roll.active_clip_id {
            Some(id) => id,
            None => return,
        };

        match event {
            AppEvent::AddNote(mut note) => {
                note.id = self.piano_roll.alloc_note_id();
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    let new_clip = clip.with_note_added(note);
                    self.update_clip(new_clip);
                    self.send_command(EngineCommand::AddNote { clip_id, note });
                }
            }
            AppEvent::RemoveNote(note_id) => {
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    let new_clip = clip.with_note_removed(*note_id);
                    self.update_clip(new_clip);
                    self.send_command(EngineCommand::RemoveNote { clip_id, note_id: *note_id });
                }
            }
            AppEvent::MoveNote { note_id, new_start, new_pitch } => {
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    if let Some(note) = clip.notes.iter().find(|n| n.id == *note_id) {
                        let updated = Note { start_tick: *new_start, pitch: *new_pitch, ..*note };
                        let new_clip = clip.with_note_updated(updated);
                        self.update_clip(new_clip);
                        self.send_command(EngineCommand::MoveNote {
                            clip_id, note_id: *note_id, new_start: *new_start, new_pitch: *new_pitch,
                        });
                    }
                }
            }
            AppEvent::ResizeNote { note_id, new_duration } => {
                if let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) {
                    if let Some(note) = clip.notes.iter().find(|n| n.id == *note_id) {
                        let updated = Note { duration_ticks: *new_duration, ..*note };
                        let new_clip = clip.with_note_updated(updated);
                        self.update_clip(new_clip);
                        self.send_command(EngineCommand::ResizeNote {
                            clip_id, note_id: *note_id, new_duration: *new_duration,
                        });
                    }
                }
            }
            AppEvent::PreviewNoteOn { note, velocity } => {
                self.send_command(EngineCommand::NoteOn { channel: 0, note: *note, velocity: *velocity });
            }
            AppEvent::PreviewNoteOff { note } => {
                self.send_command(EngineCommand::NoteOff { channel: 0, note: *note, velocity: 0 });
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
                let timer = cx.add_timer(
                    Duration::from_millis(16),
                    None,
                    |cx, action| {
                        if let TimerAction::Tick(_) = action {
                            cx.emit(AppEvent::PollEngine);
                        }
                    },
                );
                cx.start_timer(timer);
            }

            // Transport
            AppEvent::Play | AppEvent::Stop | AppEvent::Record | AppEvent::Pause
            | AppEvent::SetTempo(_) | AppEvent::SetPosition(_) | AppEvent::ToggleLoop => {
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
                    self.device_status_text = match device_manager.status() {
                        ma_core::device::DeviceStatus::Active { output_device, actual_sample_rate, actual_buffer_size, .. } =>
                            format!("{output_device} ({actual_sample_rate} Hz, {actual_buffer_size} buf)"),
                        _ => "Offline".into(),
                    };
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
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                    track.volume = *volume;
                }
                self.send_command(EngineCommand::SetTrackVolume { track_id: *track_id, volume: *volume });
            }
            AppEvent::SetTrackPan { track_id, pan } => {
                if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                    track.pan = *pan;
                }
                self.send_command(EngineCommand::SetTrackPan { track_id: *track_id, pan: *pan });
            }
            AppEvent::ToggleMute(track_id) => {
                let new_mute = if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                    track.mute = !track.mute;
                    Some(track.mute)
                } else {
                    None
                };
                if let Some(mute) = new_mute {
                    self.send_command(EngineCommand::SetTrackMute { track_id: *track_id, mute });
                }
            }
            AppEvent::ToggleSolo(track_id) => {
                let new_solo = if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                    track.solo = !track.solo;
                    Some(track.solo)
                } else {
                    None
                };
                if let Some(solo) = new_solo {
                    self.send_command(EngineCommand::SetTrackSolo { track_id: *track_id, solo });
                }
            }

            // Arrangement scroll/zoom
            AppEvent::ScrollArrangementX(dx) => { self.arrangement.scroll_x += dx; }
            AppEvent::ScrollArrangementY(dy) => { self.arrangement.scroll_y += dy; }
            AppEvent::ZoomArrangement(factor) => {
                self.arrangement.zoom_x = (self.arrangement.zoom_x * factor).clamp(0.001, 1.0);
            }

            // Piano roll
            AppEvent::AddNote(_) | AppEvent::RemoveNote(_) | AppEvent::MoveNote { .. }
            | AppEvent::ResizeNote { .. } | AppEvent::PreviewNoteOn { .. }
            | AppEvent::PreviewNoteOff { .. } | AppEvent::UpdateInteraction(_)
            | AppEvent::SetQuantize(_) => {
                self.dispatch_piano_roll(app_event);
            }
        });
    }
}
