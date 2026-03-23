//! Mixer View — horizontal row of channel strips with faders, meters, mute/solo.

use crate::state::app_state::AppState;
use crate::types::track::TrackId;
use crate::widgets::channel_strip::{ChannelStrip, ChannelStripAction};

/// Actions emitted by the mixer view.
#[derive(Debug, Clone)]
pub enum MixerAction {
    SetVolume { track_id: TrackId, volume: f32 },
    SetPan { track_id: TrackId, pan: f32 },
    ToggleMute(TrackId),
    ToggleSolo(TrackId),
}

/// Mixer view response.
pub struct MixerResponse {
    pub actions: Vec<MixerAction>,
}

/// The mixer view — shows all tracks as vertical channel strips.
pub struct MixerView<'a> {
    state: &'a AppState,
}

impl<'a> MixerView<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) -> MixerResponse {
        let mut actions = Vec::new();

        egui::ScrollArea::horizontal()
            .id_salt("mixer_scroll")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;

                    for track in &self.state.tracks {
                        let peaks = self.state.mixer.get_meter(track.id);

                        // Visual separator frame
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(32, 32, 32))
                            .inner_margin(4.0)
                            .corner_radius(2.0)
                            .show(ui, |ui| {
                                let resp = ChannelStrip::new(track, peaks).show(ui);

                                for action in resp.actions {
                                    let track_id = resp.track_id;
                                    match action {
                                        ChannelStripAction::SetVolume(v) => {
                                            actions.push(MixerAction::SetVolume {
                                                track_id,
                                                volume: v,
                                            });
                                        }
                                        ChannelStripAction::SetPan(p) => {
                                            actions.push(MixerAction::SetPan {
                                                track_id,
                                                pan: p,
                                            });
                                        }
                                        ChannelStripAction::ToggleMute => {
                                            actions.push(MixerAction::ToggleMute(track_id));
                                        }
                                        ChannelStripAction::ToggleSolo => {
                                            actions.push(MixerAction::ToggleSolo(track_id));
                                        }
                                    }
                                }
                            });
                    }

                    // Master bus placeholder
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(40, 32, 32))
                        .inner_margin(4.0)
                        .corner_radius(2.0)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(60.0);
                                ui.label(
                                    egui::RichText::new("Master")
                                        .size(11.0)
                                        .color(egui::Color32::WHITE),
                                );
                                ui.add_space(4.0);

                                // CPU load indicator
                                let cpu_pct = self.state.mixer.cpu_load * 100.0;
                                let cpu_color = if cpu_pct > 80.0 {
                                    egui::Color32::RED
                                } else if cpu_pct > 50.0 {
                                    egui::Color32::YELLOW
                                } else {
                                    egui::Color32::GREEN
                                };
                                ui.label(
                                    egui::RichText::new(format!("CPU: {cpu_pct:.0}%"))
                                        .size(9.0)
                                        .color(cpu_color),
                                );
                            });
                        });

                    if self.state.tracks.is_empty() {
                        ui.label(
                            egui::RichText::new("No tracks")
                                .color(egui::Color32::from_rgb(100, 100, 100)),
                        );
                    }
                });
            });

        MixerResponse { actions }
    }
}
