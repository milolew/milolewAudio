//! Piano Roll View — MIDI note editor with grid, keyboard strip, and mouse interaction FSM.
//!
//! Interaction states: Idle → DrawingNote / DraggingNote / ResizingNote → Idle
//! All state transitions emit PianoRollActions that are handled by the app layer.

use crate::state::app_state::AppState;
use crate::state::piano_roll_state::{PianoRollInteraction, PianoRollState, ResizeEdge};
use crate::types::midi::{Note, NoteId};
use crate::types::time::{Tick, PPQN};
use crate::types::track::ClipState;
use crate::widgets::keyboard_strip::{KeyboardAction, KeyboardStrip};
use crate::widgets::timeline_ruler::TimelineRuler;

/// Actions emitted by the piano roll.
#[derive(Debug, Clone)]
pub enum PianoRollAction {
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
    PreviewNoteOn {
        note: u8,
        velocity: u8,
    },
    PreviewNoteOff {
        note: u8,
    },
    /// Piano roll interaction state changed — caller must update PianoRollState.
    UpdateInteraction(PianoRollInteraction),
    /// Change the quantize grid.
    SetQuantize(crate::types::time::QuantizeGrid),
}

/// Piano roll response.
pub struct PianoRollResponse {
    pub actions: Vec<PianoRollAction>,
}

/// The piano roll MIDI editor view.
pub struct PianoRollView<'a> {
    state: &'a AppState,
}

impl<'a> PianoRollView<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) -> PianoRollResponse {
        let mut actions = Vec::new();

        let clip_id = match self.state.piano_roll.active_clip_id {
            Some(id) => id,
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "No clip selected — double-click a MIDI clip in the arrangement",
                        )
                        .color(egui::Color32::from_rgb(100, 100, 100))
                        .size(14.0),
                    );
                });
                return PianoRollResponse { actions };
            }
        };

        let clip = match self.state.clip(clip_id) {
            Some(c) => c,
            None => {
                ui.label("Clip not found");
                return PianoRollResponse { actions };
            }
        };

        let pr = &self.state.piano_roll;
        let available = ui.available_size();
        let keyboard_width = 48.0;
        let ruler_height = 24.0;
        let grid_width = available.x - keyboard_width;
        let grid_height = available.y - ruler_height;
        let visible_rows = pr.visible_rows(grid_height);

        // Top row: empty corner + timeline ruler
        ui.horizontal(|ui| {
            ui.allocate_space(egui::vec2(keyboard_width, ruler_height));
            TimelineRuler::new(
                pr.scroll_x,
                pr.zoom_x,
                self.state.transport.time_signature,
                self.state.transport.position,
            )
            .show(ui, grid_width);
        });

        // Main area: keyboard strip + grid
        ui.horizontal(|ui| {
            // Keyboard strip
            let kb_resp = KeyboardStrip::new(pr.scroll_y, pr.note_height, visible_rows).show(ui);
            for ka in kb_resp.actions {
                match ka {
                    KeyboardAction::NoteOn { note, velocity } => {
                        actions.push(PianoRollAction::PreviewNoteOn { note, velocity });
                    }
                    KeyboardAction::NoteOff { note } => {
                        actions.push(PianoRollAction::PreviewNoteOff { note });
                    }
                }
            }

            // Grid area
            let (grid_rect, grid_response) = ui.allocate_exact_size(
                egui::vec2(grid_width, grid_height),
                egui::Sense::click_and_drag(),
            );

            if ui.is_rect_visible(grid_rect) {
                self.paint_grid(ui, grid_rect, pr);
                self.paint_notes(ui, grid_rect, pr, clip);
                self.paint_playhead(ui, grid_rect, pr);
                self.paint_ghost_note(ui, grid_rect, pr);
            }

            // Handle interaction
            self.handle_grid_interaction(
                &grid_response,
                grid_rect,
                pr,
                clip,
                &mut actions,
            );
        });

        // Quantize selector at bottom
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Grid:")
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            );
            let q = pr.quantize;
            let labels = ["Off", "1/4", "1/8", "1/16", "1/32"];
            let values = [
                crate::types::time::QuantizeGrid::Off,
                crate::types::time::QuantizeGrid::Quarter,
                crate::types::time::QuantizeGrid::Eighth,
                crate::types::time::QuantizeGrid::Sixteenth,
                crate::types::time::QuantizeGrid::ThirtySecond,
            ];
            for (label, value) in labels.iter().zip(values.iter()) {
                let selected = q == *value;
                let text = if selected {
                    egui::RichText::new(*label).size(10.0).strong()
                } else {
                    egui::RichText::new(*label)
                        .size(10.0)
                        .color(egui::Color32::GRAY)
                };
                if ui.add(egui::Button::new(text).small()).clicked() {
                    actions.push(PianoRollAction::SetQuantize(*value));
                }
            }

            ui.separator();
            ui.label(
                egui::RichText::new(format!("Velocity: {}", pr.default_velocity))
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            );
        });

        PianoRollResponse { actions }
    }

    /// Paint the background grid lines.
    fn paint_grid(&self, ui: &egui::Ui, rect: egui::Rect, pr: &PianoRollState) {
        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(25, 25, 30));

        let visible_rows = pr.visible_rows(rect.height());

        // Horizontal lines (pitch rows)
        for i in 0..=visible_rows {
            let pitch = pr.scroll_y.saturating_sub(i);
            let y = rect.top() + i as f32 * pr.note_height;

            let is_c = pitch % 12 == 0;
            let is_black_key = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);

            // Alternating row background for black keys
            if is_black_key {
                let row_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), y),
                    egui::vec2(rect.width(), pr.note_height),
                );
                painter.rect_filled(
                    row_rect,
                    0.0,
                    egui::Color32::from_rgb(20, 20, 25),
                );
            }

            // Row separator
            let line_color = if is_c {
                egui::Color32::from_rgb(60, 60, 60)
            } else {
                egui::Color32::from_rgb(35, 35, 40)
            };
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(if is_c { 1.0 } else { 0.5 }, line_color),
            );
        }

        // Vertical lines (time grid)
        let grid_ticks = pr.quantize.ticks();
        let start_tick = pr.scroll_x as Tick;
        let end_tick = start_tick + (rect.width() as f64 / pr.zoom_x) as Tick;
        let first_grid = (start_tick / grid_ticks) * grid_ticks;

        let ticks_per_bar = PPQN * self.state.transport.time_signature.numerator as i64;
        let ticks_per_beat = PPQN;

        let mut tick = first_grid;
        while tick <= end_tick {
            let x = rect.left() + pr.tick_to_x(tick);

            if x >= rect.left() && x <= rect.right() {
                let is_bar = tick % ticks_per_bar == 0;
                let is_beat = tick % ticks_per_beat == 0;

                let (width, color) = if is_bar {
                    (1.0, egui::Color32::from_rgb(70, 70, 70))
                } else if is_beat {
                    (0.5, egui::Color32::from_rgb(50, 50, 55))
                } else {
                    (0.5, egui::Color32::from_rgb(35, 35, 40))
                };

                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(width, color),
                );
            }

            tick += grid_ticks;
        }
    }

    /// Paint MIDI notes as colored rectangles.
    fn paint_notes(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        pr: &PianoRollState,
        clip: &ClipState,
    ) {
        let painter = ui.painter_at(rect);

        for note in &clip.notes {
            let note_rect = self.note_to_rect(rect, pr, note);

            // Skip if off-screen
            if !note_rect.intersects(rect) {
                continue;
            }

            let is_selected = pr.selected_notes.contains(&note.id);
            let is_being_dragged = matches!(
                &pr.interaction,
                PianoRollInteraction::DraggingNote { note_id, .. }
                    if *note_id == note.id
            );

            let base_color = egui::Color32::from_rgb(100, 160, 255);
            let fill_color = if is_selected || is_being_dragged {
                egui::Color32::from_rgb(140, 200, 255)
            } else {
                base_color
            };

            // Velocity → alpha
            let alpha = 100 + (note.velocity as u16 * 155 / 127) as u8;
            let fill_with_alpha = egui::Color32::from_rgba_unmultiplied(
                fill_color.r(),
                fill_color.g(),
                fill_color.b(),
                alpha,
            );

            painter.rect_filled(note_rect, 2.0, fill_with_alpha);
            painter.rect_stroke(
                note_rect,
                2.0,
                egui::Stroke::new(
                    if is_selected { 1.5 } else { 0.5 },
                    egui::Color32::WHITE,
                ),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// Paint the playhead vertical line.
    fn paint_playhead(&self, ui: &egui::Ui, rect: egui::Rect, pr: &PianoRollState) {
        let painter = ui.painter_at(rect);
        let x = rect.left() + pr.tick_to_x(self.state.transport.position);

        if x >= rect.left() && x <= rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 100, 100)),
            );
        }
    }

    /// Paint a ghost note while drawing.
    fn paint_ghost_note(&self, ui: &egui::Ui, rect: egui::Rect, pr: &PianoRollState) {
        if let PianoRollInteraction::DrawingNote {
            start_tick,
            pitch,
            current_end_tick,
            ..
        } = &pr.interaction
        {
            let painter = ui.painter_at(rect);

            let x_start = rect.left() + pr.tick_to_x(*start_tick);
            let x_end = rect.left() + pr.tick_to_x(*current_end_tick);
            let y = pr.pitch_to_y(*pitch, rect.top());

            let ghost_rect = egui::Rect::from_min_max(
                egui::pos2(x_start.min(x_end), y),
                egui::pos2(x_start.max(x_end), y + pr.note_height),
            );

            painter.rect_filled(
                ghost_rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(100, 200, 255, 100),
            );
            painter.rect_stroke(
                ghost_rect,
                2.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 200, 255)),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// Convert a Note to a screen rectangle.
    fn note_to_rect(
        &self,
        grid_rect: egui::Rect,
        pr: &PianoRollState,
        note: &Note,
    ) -> egui::Rect {
        let x_start = grid_rect.left() + pr.tick_to_x(note.start_tick);
        let x_end = grid_rect.left() + pr.tick_to_x(note.end_tick());
        let y = pr.pitch_to_y(note.pitch, grid_rect.top());

        egui::Rect::from_min_max(
            egui::pos2(x_start, y),
            egui::pos2(x_end, y + pr.note_height),
        )
    }

    /// Handle mouse interaction on the grid — the FSM core.
    fn handle_grid_interaction(
        &self,
        response: &egui::Response,
        rect: egui::Rect,
        pr: &PianoRollState,
        clip: &ClipState,
        actions: &mut Vec<PianoRollAction>,
    ) {
        let pointer_pos = response.interact_pointer_pos().or(response.hover_pos());

        match &pr.interaction {
            PianoRollInteraction::Idle => {
                self.handle_idle(response, rect, pr, clip, actions, pointer_pos);
            }
            PianoRollInteraction::DrawingNote {
                start_tick,
                pitch,
                velocity,
                ..
            } => {
                let start_tick = *start_tick;
                let pitch = *pitch;
                let velocity = *velocity;
                self.handle_drawing(
                    response, rect, pr, actions, pointer_pos, start_tick, pitch, velocity,
                );
            }
            PianoRollInteraction::DraggingNote {
                note_id,
                original_start,
                original_pitch,
                drag_offset_tick,
                drag_offset_pitch,
            } => {
                let note_id = *note_id;
                let original_start = *original_start;
                let original_pitch = *original_pitch;
                let drag_offset_tick = *drag_offset_tick;
                let drag_offset_pitch = *drag_offset_pitch;
                self.handle_dragging(
                    response,
                    rect,
                    pr,
                    actions,
                    pointer_pos,
                    note_id,
                    original_start,
                    original_pitch,
                    drag_offset_tick,
                    drag_offset_pitch,
                );
            }
            PianoRollInteraction::ResizingNote {
                note_id,
                original_duration,
                edge,
            } => {
                let note_id = *note_id;
                let original_duration = *original_duration;
                let edge = *edge;
                self.handle_resizing(
                    response,
                    rect,
                    pr,
                    clip,
                    actions,
                    pointer_pos,
                    note_id,
                    original_duration,
                    edge,
                );
            }
            PianoRollInteraction::Selecting { .. } => {
                // Selection box — stub for now
                if response.drag_stopped() {
                    actions.push(PianoRollAction::UpdateInteraction(
                        PianoRollInteraction::Idle,
                    ));
                }
            }
        }
    }

    /// Handle Idle state: detect what the user clicked on.
    fn handle_idle(
        &self,
        response: &egui::Response,
        rect: egui::Rect,
        pr: &PianoRollState,
        clip: &ClipState,
        actions: &mut Vec<PianoRollAction>,
        pointer_pos: Option<egui::Pos2>,
    ) {
        let Some(pos) = pointer_pos else { return };
        if !rect.contains(pos) {
            return;
        }

        // Right-click to delete note
        if response.secondary_clicked() {
            if let Some(hit_note) = self.hit_test_note(rect, pr, clip, pos) {
                actions.push(PianoRollAction::RemoveNote(hit_note.id));
                return;
            }
        }

        if !response.drag_started() && !response.clicked() {
            return;
        }

        let rel_x = pos.x - rect.left();
        let tick = pr.x_to_tick(rel_x);
        let pitch = pr.y_to_pitch(pos.y, rect.top());

        // Check if we hit an existing note
        if let Some(hit_note) = self.hit_test_note(rect, pr, clip, pos) {
            let note_rect = self.note_to_rect(rect, pr, &hit_note);

            // Check if near the right edge → resize
            let edge_threshold = 6.0;
            if pos.x > note_rect.right() - edge_threshold {
                actions.push(PianoRollAction::UpdateInteraction(
                    PianoRollInteraction::ResizingNote {
                        note_id: hit_note.id,
                        original_duration: hit_note.duration_ticks,
                        edge: ResizeEdge::Right,
                    },
                ));
                return;
            }

            // Check left edge → resize
            if pos.x < note_rect.left() + edge_threshold {
                actions.push(PianoRollAction::UpdateInteraction(
                    PianoRollInteraction::ResizingNote {
                        note_id: hit_note.id,
                        original_duration: hit_note.duration_ticks,
                        edge: ResizeEdge::Left,
                    },
                ));
                return;
            }

            // Otherwise → drag
            let offset_tick = tick - hit_note.start_tick;
            actions.push(PianoRollAction::PreviewNoteOn {
                note: hit_note.pitch,
                velocity: hit_note.velocity,
            });
            actions.push(PianoRollAction::UpdateInteraction(
                PianoRollInteraction::DraggingNote {
                    note_id: hit_note.id,
                    original_start: hit_note.start_tick,
                    original_pitch: hit_note.pitch,
                    drag_offset_tick: offset_tick,
                    drag_offset_pitch: 0,
                },
            ));
            return;
        }

        // Empty space → start drawing a new note
        let snapped_start = pr.quantize.snap_floor(tick);
        let grid_ticks = pr.quantize.ticks();
        actions.push(PianoRollAction::PreviewNoteOn {
            note: pitch,
            velocity: pr.default_velocity,
        });
        actions.push(PianoRollAction::UpdateInteraction(
            PianoRollInteraction::DrawingNote {
                start_tick: snapped_start,
                pitch,
                velocity: pr.default_velocity,
                current_end_tick: snapped_start + grid_ticks,
            },
        ));
    }

    /// Handle DrawingNote state: extend the note as the mouse moves.
    #[allow(clippy::too_many_arguments)]
    fn handle_drawing(
        &self,
        response: &egui::Response,
        rect: egui::Rect,
        pr: &PianoRollState,
        actions: &mut Vec<PianoRollAction>,
        pointer_pos: Option<egui::Pos2>,
        start_tick: Tick,
        pitch: u8,
        velocity: u8,
    ) {
        if let Some(pos) = pointer_pos {
            let rel_x = pos.x - rect.left();
            let current_tick = pr.x_to_tick(rel_x);
            let snapped_end = pr.quantize.snap(current_tick).max(start_tick + pr.quantize.ticks());

            actions.push(PianoRollAction::UpdateInteraction(
                PianoRollInteraction::DrawingNote {
                    start_tick,
                    pitch,
                    velocity,
                    current_end_tick: snapped_end,
                },
            ));
        }

        // Mouse released → finalize note
        if response.drag_stopped() || (response.clicked() && !response.dragged()) {
            let end_tick = match &pr.interaction {
                PianoRollInteraction::DrawingNote {
                    current_end_tick, ..
                } => *current_end_tick,
                _ => start_tick + pr.quantize.ticks(),
            };

            let duration = (end_tick - start_tick).max(pr.quantize.ticks());

            actions.push(PianoRollAction::AddNote(Note {
                id: NoteId(0), // App layer will assign real ID
                pitch,
                start_tick,
                duration_ticks: duration,
                velocity,
                channel: 0,
            }));

            actions.push(PianoRollAction::PreviewNoteOff { note: pitch });
            actions.push(PianoRollAction::UpdateInteraction(
                PianoRollInteraction::Idle,
            ));
        }
    }

    /// Handle DraggingNote state: move note position and pitch.
    #[allow(clippy::too_many_arguments)]
    fn handle_dragging(
        &self,
        response: &egui::Response,
        rect: egui::Rect,
        pr: &PianoRollState,
        actions: &mut Vec<PianoRollAction>,
        pointer_pos: Option<egui::Pos2>,
        note_id: NoteId,
        _original_start: Tick,
        original_pitch: u8,
        drag_offset_tick: Tick,
        _drag_offset_pitch: i8,
    ) {
        if response.drag_stopped() {
            if let Some(pos) = pointer_pos {
                let rel_x = pos.x - rect.left();
                let raw_tick = pr.x_to_tick(rel_x) - drag_offset_tick;
                let snapped_tick = pr.quantize.snap_floor(raw_tick).max(0);
                let new_pitch = pr.y_to_pitch(pos.y, rect.top());

                actions.push(PianoRollAction::MoveNote {
                    note_id,
                    new_start: snapped_tick,
                    new_pitch,
                });

                if new_pitch != original_pitch {
                    actions.push(PianoRollAction::PreviewNoteOff {
                        note: original_pitch,
                    });
                }
            }

            actions.push(PianoRollAction::UpdateInteraction(
                PianoRollInteraction::Idle,
            ));
        }
    }

    /// Handle ResizingNote state: change note duration.
    #[allow(clippy::too_many_arguments)]
    fn handle_resizing(
        &self,
        response: &egui::Response,
        rect: egui::Rect,
        pr: &PianoRollState,
        clip: &ClipState,
        actions: &mut Vec<PianoRollAction>,
        pointer_pos: Option<egui::Pos2>,
        note_id: NoteId,
        _original_duration: Tick,
        edge: ResizeEdge,
    ) {
        if response.drag_stopped() {
            if let Some(pos) = pointer_pos {
                let rel_x = pos.x - rect.left();
                let current_tick = pr.x_to_tick(rel_x);
                let snapped = pr.quantize.snap(current_tick);

                if let Some(note) = clip.notes.iter().find(|n| n.id == note_id) {
                    let min_duration = pr.quantize.ticks();

                    let new_duration = match edge {
                        ResizeEdge::Right => (snapped - note.start_tick).max(min_duration),
                        ResizeEdge::Left => {
                            // Left edge resize: effectively moves start + adjusts duration
                            (note.end_tick() - snapped).max(min_duration)
                        }
                    };

                    actions.push(PianoRollAction::ResizeNote {
                        note_id,
                        new_duration,
                    });
                }
            }

            actions.push(PianoRollAction::UpdateInteraction(
                PianoRollInteraction::Idle,
            ));
        }
    }

    /// Hit-test: find which note (if any) the pointer is over.
    fn hit_test_note(
        &self,
        grid_rect: egui::Rect,
        pr: &PianoRollState,
        clip: &ClipState,
        pos: egui::Pos2,
    ) -> Option<Note> {
        // Iterate in reverse so topmost (last-drawn) notes get hit first
        for note in clip.notes.iter().rev() {
            let note_rect = self.note_to_rect(grid_rect, pr, note);
            if note_rect.contains(pos) {
                return Some(*note);
            }
        }
        None
    }
}
