//! AppData — root vizia Model for the DAW application.
//!
//! Owns all state and the engine bridge. Delegates event handling to
//! sub-modules: dispatch, engine_adapter, demo_data.

mod app_event;
mod demo_data;
mod dispatch;
mod engine_adapter;

pub use app_event::{ActiveView, AppEvent};
pub use engine_adapter::EngineMode;

use std::time::Duration;

use vizia::prelude::*;

use crate::config::load_preferences;
use crate::engine_bridge::commands::EngineCommand;
use crate::engine_bridge::responses::EngineResponse;
use crate::state::arrangement_state::ArrangementState;
use crate::state::mixer_state::MixerState;
use crate::state::piano_roll_state::PianoRollState;
use crate::state::transport_state::TransportState;
use crate::types::track::{ClipId, ClipState, TrackId, TrackState};

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
    pub device_sample_rate: String,
    pub device_buffer_size: String,
    pub device_latency: String,
    pub show_preferences: bool,

    #[lens(ignore)]
    engine: EngineMode,

    /// Pre-allocated buffer for engine responses — reused each frame to avoid allocation.
    #[lens(ignore)]
    response_buf: Vec<EngineResponse>,
}

impl Default for AppData {
    fn default() -> Self {
        Self::new()
    }
}

impl AppData {
    /// Create AppData with demo tracks/clips and a mock engine.
    pub fn new() -> Self {
        let tracks = demo_data::create_demo_tracks();
        let track_ids: Vec<TrackId> = tracks.iter().map(|t| t.id).collect();
        let clips = demo_data::create_demo_clips();

        let prefs = load_preferences();

        let engine = engine_adapter::try_real_engine(&prefs.audio).unwrap_or_else(|e| {
            log::warn!("Real audio engine unavailable: {e}. Using mock engine.");
            engine_adapter::create_mock_engine(track_ids.clone())
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
            device_status_text,
            device_sample_rate,
            device_buffer_size,
            device_latency,
            show_preferences: false,
            engine,
            response_buf: Vec::with_capacity(64),
        }
    }

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

    fn update_clip(&mut self, updated: ClipState) {
        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == updated.id) {
            *clip = updated;
        }
    }

    fn send_command(&mut self, cmd: EngineCommand) {
        engine_adapter::send_command(&mut self.engine, cmd);
    }

    fn poll_engine(&mut self) {
        engine_adapter::poll_engine(
            &mut self.engine,
            &mut self.response_buf,
            &mut self.transport,
            &mut self.mixer,
        );
    }
}

impl Model for AppData {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _meta| match app_event {
            AppEvent::PollEngine => self.poll_engine(),

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
            | AppEvent::SetTempo(_)
            | AppEvent::SetPosition(_)
            | AppEvent::ToggleLoop => dispatch::dispatch_transport(self, app_event),

            // Preferences
            AppEvent::ShowPreferences => self.show_preferences = true,
            AppEvent::HidePreferences => self.show_preferences = false,
            AppEvent::RefreshDevices => dispatch::dispatch_preferences(self),

            // View switching
            AppEvent::SwitchView(view) => self.active_view = *view,
            AppEvent::OpenPianoRoll(clip_id) => {
                self.piano_roll.active_clip_id = Some(*clip_id);
                self.active_view = ActiveView::PianoRoll;
            }

            // Track selection
            AppEvent::SelectTrack(id) => self.arrangement.selected_track = Some(*id),

            // Mixer
            AppEvent::SetTrackVolume { .. }
            | AppEvent::SetTrackPan { .. }
            | AppEvent::ToggleMute(_)
            | AppEvent::ToggleSolo(_) => dispatch::dispatch_mixer(self, app_event),

            // Arrangement scroll/zoom
            AppEvent::ScrollArrangementX(dx) => {
                self.arrangement.scroll_x = (self.arrangement.scroll_x + dx).max(0.0);
            }
            AppEvent::ScrollArrangementY(dy) => self.arrangement.scroll_y += dy,
            AppEvent::ZoomArrangement(factor) => {
                self.arrangement.zoom_x = (self.arrangement.zoom_x * factor).clamp(0.001, 1.0);
            }

            // Piano roll editing
            AppEvent::AddNote(_)
            | AppEvent::RemoveNote(_)
            | AppEvent::MoveNote { .. }
            | AppEvent::ResizeNote { .. }
            | AppEvent::PreviewNoteOn { .. }
            | AppEvent::PreviewNoteOff { .. }
            | AppEvent::UpdateInteraction(_)
            | AppEvent::SetQuantize(_) => dispatch::dispatch_piano_roll(self, app_event),

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
        });
    }
}
