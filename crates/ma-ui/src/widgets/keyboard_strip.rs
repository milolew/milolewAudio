//! Vertical piano keyboard strip for the piano roll left sidebar.

use crate::types::midi::note_name;

/// Actions from the keyboard strip (clicking a key).
#[derive(Debug, Clone)]
pub enum KeyboardAction {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
}

/// Keyboard strip response.
pub struct KeyboardStripResponse {
    pub actions: Vec<KeyboardAction>,
}

/// Vertical piano keyboard showing note names.
pub struct KeyboardStrip {
    scroll_y: u8,
    note_height: f32,
    width: f32,
    visible_rows: u8,
}

impl KeyboardStrip {
    pub fn new(scroll_y: u8, note_height: f32, visible_rows: u8) -> Self {
        Self {
            scroll_y,
            note_height,
            width: 48.0,
            visible_rows,
        }
    }

    pub fn show(self, ui: &mut egui::Ui) -> KeyboardStripResponse {
        let height = self.visible_rows as f32 * self.note_height;
        let desired_size = egui::vec2(self.width, height);
        let (rect, response) =
            ui.allocate_exact_size(desired_size, egui::Sense::click());

        let mut actions = Vec::new();

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            for i in 0..self.visible_rows {
                let pitch = self.scroll_y.saturating_sub(i);
                if pitch > 127 {
                    continue;
                }

                let y = rect.top() + i as f32 * self.note_height;
                let key_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), y),
                    egui::vec2(self.width, self.note_height),
                );

                let is_black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
                let bg_color = if is_black {
                    egui::Color32::from_rgb(30, 30, 30)
                } else {
                    egui::Color32::from_rgb(60, 60, 60)
                };

                painter.rect_filled(key_rect, 0.0, bg_color);

                // Border between keys
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), y + self.note_height),
                        egui::pos2(rect.right(), y + self.note_height),
                    ],
                    egui::Stroke::new(0.5, egui::Color32::from_rgb(50, 50, 50)),
                );

                // Note name on C notes or every key if zoomed in enough
                let is_c = pitch % 12 == 0;
                if is_c || self.note_height > 16.0 {
                    let name = note_name(pitch);
                    let text_color = if is_c {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(120, 120, 120)
                    };
                    painter.text(
                        egui::pos2(rect.right() - 4.0, y + self.note_height / 2.0),
                        egui::Align2::RIGHT_CENTER,
                        name,
                        egui::FontId::proportional(9.0),
                        text_color,
                    );
                }
            }
        }

        // Handle click on keyboard to preview note
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let row = ((pos.y - rect.top()) / self.note_height) as u8;
                let pitch = self.scroll_y.saturating_sub(row);
                if pitch <= 127 {
                    actions.push(KeyboardAction::NoteOn {
                        note: pitch,
                        velocity: 100,
                    });
                }
            }
        }

        KeyboardStripResponse { actions }
    }
}
