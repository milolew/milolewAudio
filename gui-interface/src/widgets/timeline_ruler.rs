//! Timeline ruler widget — shows bar/beat markers at the top.

use crate::types::time::{tick_to_bbt, Tick, TimeSignature, PPQN};

/// Timeline ruler — horizontal bar/beat marker strip.
pub struct TimelineRuler {
    scroll_x: f64,
    zoom_x: f64,
    time_signature: TimeSignature,
    playhead: Tick,
}

impl TimelineRuler {
    pub fn new(scroll_x: f64, zoom_x: f64, time_signature: TimeSignature, playhead: Tick) -> Self {
        Self {
            scroll_x,
            zoom_x,
            time_signature,
            playhead,
        }
    }

    pub fn show(self, ui: &mut egui::Ui, width: f32) -> egui::Response {
        let height = 24.0;
        let desired_size = egui::vec2(width, height);
        let (rect, response) =
            ui.allocate_exact_size(desired_size, egui::Sense::click());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            // Background
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(35, 35, 35));

            let ticks_per_bar = PPQN * self.time_signature.numerator as i64;
            let ticks_per_beat = PPQN;

            // Calculate visible tick range
            let start_tick = self.scroll_x as Tick;
            let end_tick = start_tick + (width as f64 / self.zoom_x) as Tick;

            // Draw bar lines and numbers
            let first_bar_tick = (start_tick / ticks_per_bar) * ticks_per_bar;
            let mut tick = first_bar_tick;

            while tick <= end_tick {
                let x = rect.left() + ((tick as f64 - self.scroll_x) * self.zoom_x) as f32;

                if x >= rect.left() && x <= rect.right() {
                    let is_bar = tick % ticks_per_bar == 0;

                    if is_bar {
                        // Bar line
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 120, 120)),
                        );

                        // Bar number
                        let bbt = tick_to_bbt(tick, self.time_signature);
                        painter.text(
                            egui::pos2(x + 4.0, rect.top() + 4.0),
                            egui::Align2::LEFT_TOP,
                            format!("{}", bbt.bar),
                            egui::FontId::proportional(11.0),
                            egui::Color32::LIGHT_GRAY,
                        );
                    } else {
                        // Beat tick (thinner)
                        painter.line_segment(
                            [
                                egui::pos2(x, rect.bottom() - 8.0),
                                egui::pos2(x, rect.bottom()),
                            ],
                            egui::Stroke::new(0.5, egui::Color32::from_rgb(80, 80, 80)),
                        );
                    }
                }

                tick += ticks_per_beat;
            }

            // Playhead marker
            let playhead_x =
                rect.left() + ((self.playhead as f64 - self.scroll_x) * self.zoom_x) as f32;
            if playhead_x >= rect.left() && playhead_x <= rect.right() {
                painter.line_segment(
                    [
                        egui::pos2(playhead_x, rect.top()),
                        egui::pos2(playhead_x, rect.bottom()),
                    ],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 100, 100)),
                );
                // Playhead triangle
                let tri_size = 6.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(playhead_x, rect.top()),
                        egui::pos2(playhead_x - tri_size, rect.top() - tri_size),
                        egui::pos2(playhead_x + tri_size, rect.top() - tri_size),
                    ],
                    egui::Color32::from_rgb(255, 100, 100),
                    egui::Stroke::NONE,
                ));
            }
        }

        response
    }
}
