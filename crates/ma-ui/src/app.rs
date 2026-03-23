//! DawApp — main application implementing eframe::App.
//!
//! Owns all state, coordinates the unidirectional data flow:
//! poll responses → update state → render views → dispatch actions → send commands.

use eframe::egui;
use uuid::Uuid;

use ma_audio_engine::device_manager::AudioDeviceManager;
use ma_audio_engine::engine::EngineConfig;
use ma_core::commands::EngineCommand as CoreCommand;
use ma_core::device::AudioDeviceConfig;

use crate::engine_bridge::bridge::{create_bridge, EngineBridge};
use crate::engine_bridge::commands::EngineCommand;
use crate::engine_bridge::mock_engine::{spawn_mock_engine, MockEngineHandle};
use crate::engine_bridge::real_bridge::RealEngineBridge;
use crate::engine_bridge::responses::EngineResponse;
use crate::state::app_state::{ActiveView, AppState};
use crate::types::midi::NoteId;
use crate::types::track::{ClipId, ClipState, TrackId, TrackState};
use crate::types::time::PPQN;
use crate::views::arrangement_view::{ArrangementAction, ArrangementView};
use crate::views::mixer_view::{MixerAction, MixerView};
use crate::views::piano_roll_view::{PianoRollAction, PianoRollView};
use crate::widgets::transport_bar::{TransportAction, TransportBar};

/// Create a deterministic UUID for demo data (stable across restarts).
fn demo_id(n: u64) -> Uuid {
    Uuid::from_u64_pair(0, n)
}

/// Engine connection mode — real audio hardware or mock for development.
pub enum EngineMode {
    /// Connected to real audio hardware via cpal.
    Real {
        device_manager: AudioDeviceManager,
        bridge: RealEngineBridge,
    },
    /// Mock engine for standalone GUI development / no audio device.
    Mock {
        bridge: EngineBridge,
        _handle: MockEngineHandle,
    },
}

/// The main DAW application.
pub struct DawApp {
    state: AppState,
    engine: EngineMode,
}

impl DawApp {
    /// Create a new DawApp with demo data and mock engine.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Demo tracks
        let tracks = vec![
            TrackState::new_midi(TrackId(demo_id(1)), "Melody", [100, 160, 255]),
            TrackState::new_midi(TrackId(demo_id(2)), "Bass", [255, 140, 80]),
            TrackState::new_audio(TrackId(demo_id(3)), "Drums", [80, 220, 120]),
            TrackState::new_midi(TrackId(demo_id(4)), "Pad", [200, 100, 255]),
        ];

        let track_ids: Vec<TrackId> = tracks.iter().map(|t| t.id).collect();

        // Demo clips with some notes
        let clips = vec![
            ClipState {
                id: ClipId(demo_id(1)),
                track_id: TrackId(demo_id(1)),
                start_tick: 0,
                duration_ticks: PPQN * 8, // 2 bars
                name: "Melody A".into(),
                notes: vec![
                    crate::types::midi::Note {
                        id: NoteId(100),
                        pitch: 60,
                        start_tick: 0,
                        duration_ticks: PPQN / 2,
                        velocity: 100,
                        channel: 0,
                    },
                    crate::types::midi::Note {
                        id: NoteId(101),
                        pitch: 64,
                        start_tick: PPQN / 2,
                        duration_ticks: PPQN / 2,
                        velocity: 90,
                        channel: 0,
                    },
                    crate::types::midi::Note {
                        id: NoteId(102),
                        pitch: 67,
                        start_tick: PPQN,
                        duration_ticks: PPQN,
                        velocity: 110,
                        channel: 0,
                    },
                    crate::types::midi::Note {
                        id: NoteId(103),
                        pitch: 72,
                        start_tick: PPQN * 2,
                        duration_ticks: PPQN * 2,
                        velocity: 80,
                        channel: 0,
                    },
                ],
            },
            ClipState {
                id: ClipId(demo_id(2)),
                track_id: TrackId(demo_id(2)),
                start_tick: 0,
                duration_ticks: PPQN * 8,
                name: "Bass Line".into(),
                notes: vec![
                    crate::types::midi::Note {
                        id: NoteId(200),
                        pitch: 36,
                        start_tick: 0,
                        duration_ticks: PPQN * 2,
                        velocity: 120,
                        channel: 0,
                    },
                    crate::types::midi::Note {
                        id: NoteId(201),
                        pitch: 40,
                        start_tick: PPQN * 2,
                        duration_ticks: PPQN * 2,
                        velocity: 110,
                        channel: 0,
                    },
                ],
            },
            ClipState {
                id: ClipId(demo_id(3)),
                track_id: TrackId(demo_id(3)),
                start_tick: 0,
                duration_ticks: PPQN * 16,
                name: "Drum Loop".into(),
                notes: Vec::new(), // Audio clip — no MIDI notes
            },
            ClipState {
                id: ClipId(demo_id(4)),
                track_id: TrackId(demo_id(4)),
                start_tick: PPQN * 4,
                duration_ticks: PPQN * 12,
                name: "Pad Chords".into(),
                notes: vec![
                    crate::types::midi::Note {
                        id: NoteId(300),
                        pitch: 60,
                        start_tick: PPQN * 4,
                        duration_ticks: PPQN * 4,
                        velocity: 70,
                        channel: 0,
                    },
                    crate::types::midi::Note {
                        id: NoteId(301),
                        pitch: 64,
                        start_tick: PPQN * 4,
                        duration_ticks: PPQN * 4,
                        velocity: 70,
                        channel: 0,
                    },
                    crate::types::midi::Note {
                        id: NoteId(302),
                        pitch: 67,
                        start_tick: PPQN * 4,
                        duration_ticks: PPQN * 4,
                        velocity: 70,
                        channel: 0,
                    },
                ],
            },
        ];

        let app_state = AppState {
            tracks,
            clips,
            piano_roll: crate::state::piano_roll_state::PianoRollState {
                next_note_id: 1000, // Start after demo IDs
                ..Default::default()
            },
            ..Default::default()
        };

        // Try real audio engine first, fallback to mock
        let engine = Self::try_real_engine().unwrap_or_else(|e| {
            log::warn!("Real audio engine unavailable: {e}. Using mock engine.");
            let (bridge, endpoint) = create_bridge();
            let handle = spawn_mock_engine(endpoint, track_ids.clone());
            EngineMode::Mock {
                bridge,
                _handle: handle,
            }
        });

        Self {
            state: app_state,
            engine,
        }
    }

    /// Attempt to start the real audio engine with default device.
    fn try_real_engine() -> Result<EngineMode, String> {
        let mut device_manager = AudioDeviceManager::new();
        device_manager.enumerate_devices();

        let device_config = AudioDeviceConfig::default();
        let engine_config = EngineConfig::default();

        let handle = device_manager
            .apply_config(device_config, engine_config)
            .map_err(|e| e.to_string())?;

        let bridge = RealEngineBridge::new(handle);
        Ok(EngineMode::Real {
            device_manager,
            bridge,
        })
    }

    /// Step 1: Poll engine responses and fold into state.
    fn poll_engine(&mut self) {
        let responses = match &mut self.engine {
            EngineMode::Real { bridge, .. } => bridge.poll_responses(),
            EngineMode::Mock { bridge, .. } => bridge.poll_responses(),
        };
        for resp in responses {
            match resp {
                EngineResponse::TransportUpdate {
                    position,
                    is_playing,
                    is_recording,
                } => {
                    self.state.transport.position = position;
                    self.state.transport.is_playing = is_playing;
                    self.state.transport.is_recording = is_recording;
                }
                EngineResponse::TempoUpdate(bpm) => {
                    self.state.transport.tempo = bpm;
                }
                EngineResponse::MeterUpdate {
                    track_id,
                    peak_l,
                    peak_r,
                } => {
                    self.state.mixer.update_meter(track_id, peak_l, peak_r);
                }
                EngineResponse::CpuLoad(load) => {
                    self.state.mixer.cpu_load = load;
                }
            }
        }
    }

    /// Send a command to whichever engine is active.
    fn send_command(&mut self, cmd: EngineCommand) {
        match &mut self.engine {
            EngineMode::Real { bridge, .. } => {
                // Translate UI command to core command
                if let Some(core_cmd) = Self::translate_command(&cmd) {
                    bridge.send_command(core_cmd);
                }
            }
            EngineMode::Mock { bridge, .. } => {
                bridge.send_command(cmd);
            }
        }
    }

    /// Translate a UI engine command to a core engine command.
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
            EngineCommand::SetTrackPan { track_id, pan } => {
                Some(CoreCommand::SetTrackPan {
                    track_id: *track_id,
                    pan: *pan,
                })
            }
            EngineCommand::SetTrackMute { track_id, mute } => {
                Some(CoreCommand::SetTrackMute {
                    track_id: *track_id,
                    mute: *mute,
                })
            }
            EngineCommand::SetTrackSolo { track_id, solo } => {
                Some(CoreCommand::SetTrackSolo {
                    track_id: *track_id,
                    solo: *solo,
                })
            }
            // MIDI commands don't have core equivalents yet
            _ => None,
        }
    }

    /// Dispatch transport actions.
    fn dispatch_transport(&mut self, actions: Vec<TransportAction>) {
        for action in actions {
            match action {
                TransportAction::Play => {
                    self.send_command(EngineCommand::Play);
                }
                TransportAction::Stop => {
                    self.send_command(EngineCommand::Stop);
                }
                TransportAction::Record => {
                    self.send_command(EngineCommand::Record);
                }
                TransportAction::Pause => {
                    self.send_command(EngineCommand::Pause);
                }
                TransportAction::SetTempo(bpm) => {
                    self.send_command(EngineCommand::SetTempo(bpm));
                }
            }
        }
    }

    /// Dispatch arrangement view actions.
    fn dispatch_arrangement(&mut self, actions: Vec<ArrangementAction>) {
        for action in actions {
            match action {
                ArrangementAction::SelectTrack(id) => {
                    self.state.arrangement.selected_track = Some(id);
                }
                ArrangementAction::OpenPianoRoll(clip_id) => {
                    self.state.piano_roll.active_clip_id = Some(clip_id);
                    self.state.active_view = ActiveView::PianoRoll;
                }
                ArrangementAction::ScrollX(dx) => {
                    self.state.arrangement.scroll_x += dx;
                }
                ArrangementAction::ScrollY(dy) => {
                    self.state.arrangement.scroll_y += dy;
                }
            }
        }
    }

    /// Dispatch mixer view actions.
    fn dispatch_mixer(&mut self, actions: Vec<MixerAction>) {
        for action in actions {
            match action {
                MixerAction::SetVolume { track_id, volume } => {
                    if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.volume = volume;
                    }
                    self.send_command(EngineCommand::SetTrackVolume { track_id, volume });
                }
                MixerAction::SetPan { track_id, pan } => {
                    if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.pan = pan;
                    }
                    self.send_command(EngineCommand::SetTrackPan { track_id, pan });
                }
                MixerAction::ToggleMute(track_id) => {
                    let new_mute = if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.mute = !track.mute;
                        Some(track.mute)
                    } else {
                        None
                    };
                    if let Some(mute) = new_mute {
                        self.send_command(EngineCommand::SetTrackMute { track_id, mute });
                    }
                }
                MixerAction::ToggleSolo(track_id) => {
                    let new_solo = if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.solo = !track.solo;
                        Some(track.solo)
                    } else {
                        None
                    };
                    if let Some(solo) = new_solo {
                        self.send_command(EngineCommand::SetTrackSolo { track_id, solo });
                    }
                }
            }
        }
    }

    /// Dispatch piano roll actions.
    fn dispatch_piano_roll(&mut self, actions: Vec<PianoRollAction>) {
        let clip_id = match self.state.piano_roll.active_clip_id {
            Some(id) => id,
            None => return,
        };

        for action in actions {
            match action {
                PianoRollAction::AddNote(mut note) => {
                    note.id = self.state.piano_roll.alloc_note_id();
                    if let Some(clip) = self.state.clips.iter().find(|c| c.id == clip_id) {
                        let new_clip = clip.with_note_added(note);
                        self.state.update_clip(new_clip);
                        self.send_command(EngineCommand::AddNote { clip_id, note });
                    }
                }
                PianoRollAction::RemoveNote(note_id) => {
                    if let Some(clip) = self.state.clips.iter().find(|c| c.id == clip_id) {
                        let new_clip = clip.with_note_removed(note_id);
                        self.state.update_clip(new_clip);
                        self.send_command(EngineCommand::RemoveNote { clip_id, note_id });
                    }
                }
                PianoRollAction::MoveNote {
                    note_id,
                    new_start,
                    new_pitch,
                } => {
                    if let Some(clip) = self.state.clips.iter().find(|c| c.id == clip_id) {
                        if let Some(note) = clip.notes.iter().find(|n| n.id == note_id) {
                            let updated = crate::types::midi::Note {
                                start_tick: new_start,
                                pitch: new_pitch,
                                ..*note
                            };
                            let new_clip = clip.with_note_updated(updated);
                            self.state.update_clip(new_clip);
                            self.send_command(EngineCommand::MoveNote {
                                clip_id,
                                note_id,
                                new_start,
                                new_pitch,
                            });
                        }
                    }
                }
                PianoRollAction::ResizeNote {
                    note_id,
                    new_duration,
                } => {
                    if let Some(clip) = self.state.clips.iter().find(|c| c.id == clip_id) {
                        if let Some(note) = clip.notes.iter().find(|n| n.id == note_id) {
                            let updated = crate::types::midi::Note {
                                duration_ticks: new_duration,
                                ..*note
                            };
                            let new_clip = clip.with_note_updated(updated);
                            self.state.update_clip(new_clip);
                            self.send_command(EngineCommand::ResizeNote {
                                clip_id,
                                note_id,
                                new_duration,
                            });
                        }
                    }
                }
                PianoRollAction::PreviewNoteOn { note, velocity } => {
                    self.send_command(EngineCommand::NoteOn {
                        channel: 0,
                        note,
                        velocity,
                    });
                }
                PianoRollAction::PreviewNoteOff { note } => {
                    self.send_command(EngineCommand::NoteOff {
                        channel: 0,
                        note,
                        velocity: 0,
                    });
                }
                PianoRollAction::UpdateInteraction(new_interaction) => {
                    self.state.piano_roll.interaction = new_interaction;
                }
                PianoRollAction::SetQuantize(grid) => {
                    self.state.piano_roll.quantize = grid;
                }
            }
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request continuous repaint for transport/meter updates
        if self.state.transport.is_playing {
            ctx.request_repaint();
        }

        // Step 1: Poll engine
        self.poll_engine();

        // Step 2 & 3: Render UI and collect actions
        // Top panel: transport bar
        let transport_actions = egui::TopBottomPanel::top("transport_panel")
            .show(ctx, |ui| {
                TransportBar::new(&self.state.transport).show(ui)
            })
            .inner
            .actions;

        // View switcher tabs at top of central panel
        egui::TopBottomPanel::top("view_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let views = [
                    (ActiveView::Arrangement, "Arrangement"),
                    (ActiveView::Mixer, "Mixer"),
                    (ActiveView::PianoRoll, "Piano Roll"),
                ];
                for (view, label) in &views {
                    let selected = self.state.active_view == *view;
                    let text = if selected {
                        egui::RichText::new(*label).strong().size(12.0)
                    } else {
                        egui::RichText::new(*label)
                            .size(12.0)
                            .color(egui::Color32::GRAY)
                    };
                    if ui.add(egui::Button::new(text)).clicked() {
                        self.state.active_view = *view;
                    }
                }

                // Show active clip name if in piano roll
                if self.state.active_view == ActiveView::PianoRoll {
                    if let Some(clip_id) = self.state.piano_roll.active_clip_id {
                        if let Some(clip) = self.state.clip(clip_id) {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!("Editing: {}", clip.name))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(150, 200, 255)),
                            );
                        }
                    }
                }
            });
        });

        // Central panel: active view
        let (arrangement_actions, mixer_actions, piano_roll_actions) =
            egui::CentralPanel::default()
                .show(ctx, |ui| {
                    let mut arr_actions = Vec::new();
                    let mut mix_actions = Vec::new();
                    let mut pr_actions = Vec::new();

                    match self.state.active_view {
                        ActiveView::Arrangement => {
                            arr_actions = ArrangementView::new(&self.state).show(ui).actions;
                        }
                        ActiveView::Mixer => {
                            mix_actions = MixerView::new(&self.state).show(ui).actions;
                        }
                        ActiveView::PianoRoll => {
                            pr_actions = PianoRollView::new(&self.state).show(ui).actions;
                        }
                    }

                    (arr_actions, mix_actions, pr_actions)
                })
                .inner;

        // Step 4: Dispatch all actions
        self.dispatch_transport(transport_actions);
        self.dispatch_arrangement(arrangement_actions);
        self.dispatch_mixer(mixer_actions);
        self.dispatch_piano_roll(piano_roll_actions);
    }
}
