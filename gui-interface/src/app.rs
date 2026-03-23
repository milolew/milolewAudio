//! DawApp — main application implementing eframe::App.
//!
//! Owns all state, coordinates the unidirectional data flow:
//! poll responses → update state → render views → dispatch actions → send commands.

use eframe::egui;

use crate::engine_bridge::bridge::{create_bridge, EngineBridge};
use crate::engine_bridge::commands::EngineCommand;
use crate::engine_bridge::mock_engine::{spawn_mock_engine, MockEngineHandle};
use crate::engine_bridge::responses::EngineResponse;
use crate::state::app_state::{ActiveView, AppState};
use crate::types::midi::NoteId;
use crate::types::track::{ClipId, ClipState, TrackId, TrackState};
use crate::types::time::PPQN;
use crate::views::arrangement_view::{ArrangementAction, ArrangementView};
use crate::views::mixer_view::{MixerAction, MixerView};
use crate::views::piano_roll_view::{PianoRollAction, PianoRollView};
use crate::widgets::transport_bar::{TransportAction, TransportBar};

/// The main DAW application.
pub struct DawApp {
    state: AppState,
    bridge: EngineBridge,
    _engine_handle: MockEngineHandle,
}

impl DawApp {
    /// Create a new DawApp with demo data and mock engine.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (bridge, endpoint) = create_bridge();

        // Demo tracks
        let tracks = vec![
            TrackState::new_midi(TrackId(1), "Melody", [100, 160, 255]),
            TrackState::new_midi(TrackId(2), "Bass", [255, 140, 80]),
            TrackState::new_audio(TrackId(3), "Drums", [80, 220, 120]),
            TrackState::new_midi(TrackId(4), "Pad", [200, 100, 255]),
        ];

        let track_ids: Vec<TrackId> = tracks.iter().map(|t| t.id).collect();

        // Demo clips with some notes
        let clips = vec![
            ClipState {
                id: ClipId(1),
                track_id: TrackId(1),
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
                id: ClipId(2),
                track_id: TrackId(2),
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
                id: ClipId(3),
                track_id: TrackId(3),
                start_tick: 0,
                duration_ticks: PPQN * 16,
                name: "Drum Loop".into(),
                notes: Vec::new(), // Audio clip — no MIDI notes
            },
            ClipState {
                id: ClipId(4),
                track_id: TrackId(4),
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

        let mut app_state = AppState::default();
        app_state.tracks = tracks;
        app_state.clips = clips;
        app_state.piano_roll.next_note_id = 1000; // Start after demo IDs

        // Spawn mock engine
        let engine_handle = spawn_mock_engine(endpoint, track_ids);

        Self {
            state: app_state,
            bridge,
            _engine_handle: engine_handle,
        }
    }

    /// Step 1: Poll engine responses and fold into state.
    fn poll_engine(&mut self) {
        let responses = self.bridge.poll_responses();
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

    /// Dispatch transport actions.
    fn dispatch_transport(&mut self, actions: Vec<TransportAction>) {
        for action in actions {
            match action {
                TransportAction::Play => {
                    self.bridge.send_command(EngineCommand::Play);
                }
                TransportAction::Stop => {
                    self.bridge.send_command(EngineCommand::Stop);
                }
                TransportAction::Record => {
                    self.bridge.send_command(EngineCommand::Record);
                }
                TransportAction::Pause => {
                    self.bridge.send_command(EngineCommand::Pause);
                }
                TransportAction::SetTempo(bpm) => {
                    self.bridge.send_command(EngineCommand::SetTempo(bpm));
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
                    self.bridge
                        .send_command(EngineCommand::SetTrackVolume { track_id, volume });
                }
                MixerAction::SetPan { track_id, pan } => {
                    if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.pan = pan;
                    }
                    self.bridge
                        .send_command(EngineCommand::SetTrackPan { track_id, pan });
                }
                MixerAction::ToggleMute(track_id) => {
                    if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.mute = !track.mute;
                        self.bridge.send_command(EngineCommand::SetTrackMute {
                            track_id,
                            mute: track.mute,
                        });
                    }
                }
                MixerAction::ToggleSolo(track_id) => {
                    if let Some(track) = self.state.tracks.iter_mut().find(|t| t.id == track_id) {
                        track.solo = !track.solo;
                        self.bridge.send_command(EngineCommand::SetTrackSolo {
                            track_id,
                            solo: track.solo,
                        });
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
                        self.bridge
                            .send_command(EngineCommand::AddNote { clip_id, note });
                    }
                }
                PianoRollAction::RemoveNote(note_id) => {
                    if let Some(clip) = self.state.clips.iter().find(|c| c.id == clip_id) {
                        let new_clip = clip.with_note_removed(note_id);
                        self.state.update_clip(new_clip);
                        self.bridge
                            .send_command(EngineCommand::RemoveNote { clip_id, note_id });
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
                            self.bridge.send_command(EngineCommand::MoveNote {
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
                            self.bridge.send_command(EngineCommand::ResizeNote {
                                clip_id,
                                note_id,
                                new_duration,
                            });
                        }
                    }
                }
                PianoRollAction::PreviewNoteOn { note, velocity } => {
                    self.bridge.send_command(EngineCommand::NoteOn {
                        channel: 0,
                        note,
                        velocity,
                    });
                }
                PianoRollAction::PreviewNoteOff { note } => {
                    self.bridge.send_command(EngineCommand::NoteOff {
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
