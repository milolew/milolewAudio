//! Transport bar widget — play, stop, record, tempo, time display.

use crate::state::transport_state::TransportState;
use crate::types::time::tick_to_bbt;

/// Actions emitted by the transport bar.
#[derive(Debug, Clone)]
pub enum TransportAction {
    Play,
    Stop,
    Record,
    Pause,
    SetTempo(f64),
}

/// Transport bar response.
pub struct TransportBarResponse {
    pub actions: Vec<TransportAction>,
}

/// Transport bar widget — renders play/stop/record buttons, tempo, and position.
pub struct TransportBar<'a> {
    transport: &'a TransportState,
}

impl<'a> TransportBar<'a> {
    pub fn new(transport: &'a TransportState) -> Self {
        Self { transport }
    }

    pub fn show(self, ui: &mut egui::Ui) -> TransportBarResponse {
        let mut actions = Vec::new();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // Stop button
            let stop_color = if !self.transport.is_playing {
                egui::Color32::WHITE
            } else {
                egui::Color32::GRAY
            };
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("\u{23F9}").size(20.0).color(stop_color),
                ))
                .clicked()
            {
                actions.push(TransportAction::Stop);
            }

            // Play button
            let play_color = if self.transport.is_playing && !self.transport.is_recording {
                egui::Color32::from_rgb(100, 255, 100)
            } else {
                egui::Color32::GRAY
            };
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("\u{25B6}").size(20.0).color(play_color),
                ))
                .clicked()
            {
                actions.push(TransportAction::Play);
            }

            // Record button
            let rec_color = if self.transport.is_recording {
                egui::Color32::from_rgb(255, 60, 60)
            } else {
                egui::Color32::from_rgb(180, 60, 60)
            };
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("\u{23FA}").size(20.0).color(rec_color),
                ))
                .clicked()
            {
                actions.push(TransportAction::Record);
            }

            ui.separator();

            // Position display
            let bbt = tick_to_bbt(
                self.transport.position,
                self.transport.time_signature,
            );
            ui.label(
                egui::RichText::new(format!("{bbt}"))
                    .monospace()
                    .size(16.0),
            );

            ui.separator();

            // Tempo display/edit
            ui.label("BPM:");
            let mut tempo = self.transport.tempo;
            let tempo_response = ui.add(
                egui::DragValue::new(&mut tempo)
                    .range(20.0..=300.0)
                    .speed(0.5)
                    .fixed_decimals(1),
            );
            if tempo_response.changed() {
                actions.push(TransportAction::SetTempo(tempo));
            }

            ui.separator();

            // Time signature display
            let ts = self.transport.time_signature;
            ui.label(format!("{}/{}", ts.numerator, ts.denominator));
        });

        TransportBarResponse { actions }
    }
}
