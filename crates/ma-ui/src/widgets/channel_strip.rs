//! Channel strip widget — combines fader, meter, pan, mute/solo for one track.

use crate::state::mixer_state::MeterPeaks;
use crate::types::track::{TrackId, TrackState};

use super::fader::Fader;
use super::peak_meter::PeakMeter;

/// Actions emitted by a channel strip.
#[derive(Debug, Clone)]
pub enum ChannelStripAction {
    SetVolume(f32),
    SetPan(f32),
    ToggleMute,
    ToggleSolo,
}

/// Channel strip response.
pub struct ChannelStripResponse {
    pub track_id: TrackId,
    pub actions: Vec<ChannelStripAction>,
}

/// A single mixer channel strip.
pub struct ChannelStrip<'a> {
    track: &'a TrackState,
    peaks: MeterPeaks,
}

impl<'a> ChannelStrip<'a> {
    pub fn new(track: &'a TrackState, peaks: MeterPeaks) -> Self {
        Self { track, peaks }
    }

    pub fn show(self, ui: &mut egui::Ui) -> ChannelStripResponse {
        let mut actions = Vec::new();

        ui.vertical(|ui| {
            ui.set_width(60.0);

            // Track name
            ui.label(
                egui::RichText::new(&self.track.name)
                    .size(11.0)
                    .color(self.track.egui_color()),
            );

            ui.add_space(4.0);

            // Pan knob (simplified as horizontal drag value)
            let mut pan = self.track.pan;
            let pan_resp = ui.add(
                egui::DragValue::new(&mut pan)
                    .range(-1.0..=1.0)
                    .speed(0.01)
                    .fixed_decimals(2)
                    .prefix("P: "),
            );
            if pan_resp.changed() {
                actions.push(ChannelStripAction::SetPan(pan));
            }

            ui.add_space(4.0);

            // Fader + Meter side by side
            ui.horizontal(|ui| {
                let fader_resp = Fader::new(self.track.volume)
                    .width(26.0)
                    .height(130.0)
                    .show(ui);

                if fader_resp.changed {
                    actions.push(ChannelStripAction::SetVolume(fader_resp.value));
                }

                PeakMeter::new(self.peaks)
                    .width(14.0)
                    .height(130.0)
                    .show(ui);
            });

            ui.add_space(4.0);

            // Mute / Solo buttons
            ui.horizontal(|ui| {
                let mute_color = if self.track.mute {
                    egui::Color32::from_rgb(255, 160, 0)
                } else {
                    egui::Color32::GRAY
                };
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("M").size(12.0).color(mute_color),
                    ).min_size(egui::vec2(24.0, 20.0)))
                    .clicked()
                {
                    actions.push(ChannelStripAction::ToggleMute);
                }

                let solo_color = if self.track.solo {
                    egui::Color32::from_rgb(255, 255, 0)
                } else {
                    egui::Color32::GRAY
                };
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("S").size(12.0).color(solo_color),
                    ).min_size(egui::vec2(24.0, 20.0)))
                    .clicked()
                {
                    actions.push(ChannelStripAction::ToggleSolo);
                }
            });
        });

        ChannelStripResponse {
            track_id: self.track.id,
            actions,
        }
    }
}
