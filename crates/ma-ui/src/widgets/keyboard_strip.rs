//! Vertical piano keyboard strip for the piano roll left sidebar.
//!
//! Draws a traditional piano keyboard vertically, aligned with the piano roll
//! pitch rows. Clicking a key emits PreviewNoteOn/Off events for auditioning.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::types::midi::{is_black_key, is_c_note, note_name};

/// Vertical piano keyboard strip displayed alongside the piano roll grid.
pub struct KeyboardStrip {
    /// Pitch of the currently pressed key (for correct NoteOff on release).
    pressed_pitch: Option<u8>,
}

impl KeyboardStrip {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self {
            pressed_pitch: None,
        }
        .build(cx, |_cx| {})
    }
}

impl View for KeyboardStrip {
    fn element(&self) -> Option<&'static str> {
        Some("keyboard-strip")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        let Some(app) = cx.data::<AppData>() else {
            return;
        };

        let pr = &app.piano_roll;
        let note_height = pr.note_height;
        let scroll_y = pr.scroll_y;
        let visible_rows = pr.visible_rows(bounds.h) + 2; // draw a couple extra for partial rows

        // -- Background fill --
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, 50, 50, 50));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg_paint,
        );

        // Reusable paints
        let mut white_key_paint = vg::Paint::default();
        white_key_paint.set_color(vg::Color::from_argb(255, 232, 232, 232));
        white_key_paint.set_style(vg::PaintStyle::Fill);
        white_key_paint.set_anti_alias(true);

        let mut black_key_paint = vg::Paint::default();
        black_key_paint.set_color(vg::Color::from_argb(255, 58, 58, 58));
        black_key_paint.set_style(vg::PaintStyle::Fill);
        black_key_paint.set_anti_alias(true);

        let mut separator_paint = vg::Paint::default();
        separator_paint.set_color(vg::Color::from_argb(255, 80, 80, 80));
        separator_paint.set_style(vg::PaintStyle::Stroke);
        separator_paint.set_stroke_width(0.5 * scale);
        separator_paint.set_anti_alias(true);

        let mut c_separator_paint = vg::Paint::default();
        c_separator_paint.set_color(vg::Color::from_argb(255, 120, 120, 120));
        c_separator_paint.set_style(vg::PaintStyle::Stroke);
        c_separator_paint.set_stroke_width(1.0 * scale);
        c_separator_paint.set_anti_alias(true);

        let mut text_paint = vg::Paint::default();
        text_paint.set_color(vg::Color::from_argb(255, 40, 40, 40));
        text_paint.set_anti_alias(true);

        let font = vg::Font::default();

        // Black key visual width (shorter than white keys)
        let black_key_width = bounds.w * 0.6;

        // Draw each visible pitch row as a piano key
        for i in 0..visible_rows {
            let pitch_i32 = scroll_y as i32 - i as i32;
            if !(0..=127).contains(&pitch_i32) {
                continue;
            }
            let pitch = pitch_i32 as u8;

            let y = pr.pitch_to_y(pitch, bounds.y);
            let key_bottom = y + note_height;

            // Skip if entirely out of bounds
            if key_bottom < bounds.y || y > bounds.y + bounds.h {
                continue;
            }

            if is_black_key(pitch) {
                // Black key: draw dark rect, narrower
                canvas.draw_rect(
                    vg::Rect::from_xywh(bounds.x, y, black_key_width, note_height),
                    &black_key_paint,
                );
                // Right edge of black key has a light separator
                canvas.draw_rect(
                    vg::Rect::from_xywh(
                        bounds.x + black_key_width,
                        y,
                        bounds.w - black_key_width,
                        note_height,
                    ),
                    &white_key_paint,
                );
            } else {
                // White key: full width light background
                canvas.draw_rect(
                    vg::Rect::from_xywh(bounds.x, y, bounds.w, note_height),
                    &white_key_paint,
                );
            }

            // Horizontal separator between keys
            let sep_paint = if is_c_note(pitch) {
                &c_separator_paint
            } else {
                &separator_paint
            };
            canvas.draw_line(
                (bounds.x, key_bottom),
                (bounds.x + bounds.w, key_bottom),
                sep_paint,
            );

            // Draw note name text for C notes
            if is_c_note(pitch) {
                let label = note_name(pitch);
                let text_y = y + note_height * 0.75;
                let text_x = bounds.x + 4.0 * scale;

                // C notes are white keys, use dark text
                canvas.draw_str(label, (text_x, text_y), &font, &text_paint);
            }
        }

        // Right border of the keyboard strip
        let mut border_paint = vg::Paint::default();
        border_paint.set_color(vg::Color::from_argb(255, 100, 100, 100));
        border_paint.set_style(vg::PaintStyle::Stroke);
        border_paint.set_stroke_width(1.0 * scale);
        border_paint.set_anti_alias(true);
        canvas.draw_line(
            (bounds.x + bounds.w - 0.5, bounds.y),
            (bounds.x + bounds.w - 0.5, bounds.y + bounds.h),
            &border_paint,
        );
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let cursor_y = cx.mouse().cursor_y;

                if let Some(app) = cx.data::<AppData>() {
                    let pitch = app.piano_roll.y_to_pitch(cursor_y, bounds.y);
                    self.pressed_pitch = Some(pitch);
                    cx.emit(AppEvent::PreviewNoteOn {
                        note: pitch,
                        velocity: 100,
                    });
                }

                cx.capture();
                cx.needs_redraw(); // REDRAW: on-change — key press
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if let Some(pitch) = self.pressed_pitch.take() {
                    cx.emit(AppEvent::PreviewNoteOff { note: pitch });
                }

                cx.release();
                meta.consume();
            }
            _ => {}
        });
    }
}
