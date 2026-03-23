//! Peak meter widget — vertical VU/peak meter with stereo channels.

use crate::state::mixer_state::MeterPeaks;

/// A vertical peak meter (stereo: L+R side by side).
pub struct PeakMeter {
    peaks: MeterPeaks,
    width: f32,
    height: f32,
}

impl PeakMeter {
    pub fn new(peaks: MeterPeaks) -> Self {
        Self {
            peaks,
            width: 16.0,
            height: 150.0,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = egui::vec2(self.width, self.height);
        let (rect, response) =
            ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            // Background
            painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(20, 20, 20));

            let bar_width = (rect.width() - 3.0) / 2.0;

            // Left channel
            self.paint_bar(
                &painter,
                egui::Rect::from_min_size(rect.min, egui::vec2(bar_width, rect.height())),
                self.peaks.peak_l,
            );

            // Right channel
            self.paint_bar(
                &painter,
                egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + bar_width + 3.0, rect.min.y),
                    egui::vec2(bar_width, rect.height()),
                ),
                self.peaks.peak_r,
            );
        }

        response
    }

    fn paint_bar(&self, painter: &egui::Painter, rect: egui::Rect, level: f32) {
        let level = level.clamp(0.0, 1.0);
        let fill_height = rect.height() * level;
        let fill_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - fill_height),
            rect.max,
        );

        // Color gradient: green -> yellow -> red
        let color = if level > 0.9 {
            egui::Color32::from_rgb(255, 50, 50)
        } else if level > 0.7 {
            egui::Color32::from_rgb(255, 200, 50)
        } else {
            egui::Color32::from_rgb(80, 200, 80)
        };

        painter.rect_filled(fill_rect, 1.0, color);
    }
}
