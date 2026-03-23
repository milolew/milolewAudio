//! Arrangement View — horizontal timeline with track lanes and clips.

use crate::state::app_state::AppState;
use crate::types::time::PPQN;
use crate::types::track::{ClipId, TrackId, TrackKind};
use crate::widgets::timeline_ruler::TimelineRuler;

/// Actions emitted by the arrangement view.
#[derive(Debug, Clone)]
pub enum ArrangementAction {
    SelectTrack(TrackId),
    OpenPianoRoll(ClipId),
    ScrollX(f64),
    ScrollY(f32),
}

/// Arrangement view response.
pub struct ArrangementResponse {
    pub actions: Vec<ArrangementAction>,
}

/// The main arrangement/timeline view.
pub struct ArrangementView<'a> {
    state: &'a AppState,
}

impl<'a> ArrangementView<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) -> ArrangementResponse {
        let mut actions = Vec::new();
        let available = ui.available_size();

        // Track header width
        let header_width = 120.0;
        let timeline_width = available.x - header_width;

        // Timeline ruler at top
        ui.horizontal(|ui| {
            // Empty space for track headers column
            ui.allocate_space(egui::vec2(header_width, 24.0));

            TimelineRuler::new(
                self.state.arrangement.scroll_x,
                self.state.arrangement.zoom_x,
                self.state.transport.time_signature,
                self.state.transport.position,
            )
            .show(ui, timeline_width);
        });

        // Track lanes
        egui::ScrollArea::vertical()
            .id_salt("arrangement_scroll")
            .show(ui, |ui| {
                for track in &self.state.tracks {
                    let track_height = self.state.arrangement.track_height;

                    ui.horizontal(|ui| {
                        // Track header
                        let header_resp = ui.allocate_ui(
                            egui::vec2(header_width, track_height),
                            |ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(&track.name)
                                            .color(track.egui_color())
                                            .size(12.0),
                                    );
                                    let kind_label = match track.kind {
                                        TrackKind::Audio => "Audio",
                                        TrackKind::Midi => "MIDI",
                                    };
                                    ui.label(
                                        egui::RichText::new(kind_label)
                                            .size(9.0)
                                            .color(egui::Color32::GRAY),
                                    );
                                });
                            },
                        );
                        if header_resp.response.clicked() {
                            actions.push(ArrangementAction::SelectTrack(track.id));
                        }

                        // Track lane (clip area)
                        let (lane_rect, _lane_resp) = ui.allocate_exact_size(
                            egui::vec2(timeline_width, track_height),
                            egui::Sense::click(),
                        );

                        if ui.is_rect_visible(lane_rect) {
                            self.paint_track_lane(ui, lane_rect, track.id, &mut actions);
                        }
                    });

                    // Separator between tracks
                    ui.separator();
                }

                // Empty area below tracks
                if self.state.tracks.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("No tracks — add a track to get started")
                                .color(egui::Color32::from_rgb(100, 100, 100))
                                .size(14.0),
                        );
                    });
                }
            });

        ArrangementResponse { actions }
    }

    fn paint_track_lane(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        track_id: TrackId,
        actions: &mut Vec<ArrangementAction>,
    ) {
        let painter = ui.painter_at(rect);
        let arr = &self.state.arrangement;

        // Background
        let is_selected = arr.selected_track == Some(track_id);
        let bg = if is_selected {
            egui::Color32::from_rgb(35, 35, 45)
        } else {
            egui::Color32::from_rgb(28, 28, 28)
        };
        painter.rect_filled(rect, 0.0, bg);

        // Draw bar grid lines
        let ticks_per_bar = PPQN * self.state.transport.time_signature.numerator as i64;
        let start_tick = arr.scroll_x as i64;
        let end_tick = start_tick + (rect.width() as f64 / arr.zoom_x) as i64;
        let first_bar = (start_tick / ticks_per_bar) * ticks_per_bar;

        let mut tick = first_bar;
        while tick <= end_tick {
            let x = rect.left() + arr.tick_to_x(tick);
            if x >= rect.left() && x <= rect.right() {
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(0.5, egui::Color32::from_rgb(45, 45, 45)),
                );
            }
            tick += ticks_per_bar;
        }

        // Draw clips
        let clips = self.state.clips_for_track(track_id);
        for clip in clips {
            let clip_x_start = rect.left() + arr.tick_to_x(clip.start_tick);
            let clip_x_end = rect.left() + arr.tick_to_x(clip.end_tick());

            if clip_x_end < rect.left() || clip_x_start > rect.right() {
                continue; // Off-screen
            }

            let clip_rect = egui::Rect::from_min_max(
                egui::pos2(clip_x_start.max(rect.left()), rect.top() + 2.0),
                egui::pos2(clip_x_end.min(rect.right()), rect.bottom() - 2.0),
            );

            // Find track color
            let color = self
                .state
                .track(track_id)
                .map(|t| t.egui_color())
                .unwrap_or(egui::Color32::from_rgb(100, 100, 200));

            let clip_bg = egui::Color32::from_rgba_unmultiplied(
                color.r() / 3,
                color.g() / 3,
                color.b() / 3,
                200,
            );

            painter.rect_filled(clip_rect, 3.0, clip_bg);
            painter.rect_stroke(clip_rect, 3.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Outside);

            // Clip name
            if clip_rect.width() > 30.0 {
                painter.text(
                    egui::pos2(clip_rect.left() + 4.0, clip_rect.top() + 2.0),
                    egui::Align2::LEFT_TOP,
                    &clip.name,
                    egui::FontId::proportional(9.0),
                    egui::Color32::WHITE,
                );
            }

            // Mini note visualization for MIDI clips
            if !clip.notes.is_empty() && clip_rect.width() > 10.0 {
                self.paint_mini_notes(&painter, clip_rect, clip);
            }

            // Double-click to open piano roll
            let clip_response = ui.interact(
                clip_rect,
                egui::Id::new(("clip", clip.id)),
                egui::Sense::click(),
            );
            if clip_response.double_clicked() {
                actions.push(ArrangementAction::OpenPianoRoll(clip.id));
            }
        }

        // Playhead line
        let playhead_x = rect.left() + arr.tick_to_x(self.state.transport.position);
        if playhead_x >= rect.left() && playhead_x <= rect.right() {
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.top()),
                    egui::pos2(playhead_x, rect.bottom()),
                ],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 100, 100)),
            );
        }
    }

    fn paint_mini_notes(
        &self,
        painter: &egui::Painter,
        clip_rect: egui::Rect,
        clip: &crate::types::track::ClipState,
    ) {
        if clip.notes.is_empty() {
            return;
        }

        // Find note range for scaling
        let min_pitch = clip.notes.iter().map(|n| n.pitch).min().unwrap_or(0);
        let max_pitch = clip.notes.iter().map(|n| n.pitch).max().unwrap_or(127);
        let pitch_range = (max_pitch - min_pitch).max(1) as f32;

        let content_rect = clip_rect.shrink(3.0);

        for note in &clip.notes {
            let note_x = content_rect.left()
                + (note.start_tick - clip.start_tick) as f32
                    / clip.duration_ticks.max(1) as f32
                    * content_rect.width();
            let note_w = note.duration_ticks as f32
                / clip.duration_ticks.max(1) as f32
                * content_rect.width();
            let note_y = content_rect.bottom()
                - (note.pitch - min_pitch) as f32 / pitch_range * content_rect.height();

            let note_rect = egui::Rect::from_min_size(
                egui::pos2(note_x, note_y - 1.5),
                egui::vec2(note_w.max(1.0), 3.0),
            );

            if note_rect.intersects(content_rect) {
                painter.rect_filled(
                    note_rect,
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(200, 200, 255, 180),
                );
            }
        }
    }
}
