//! Vertical fader widget for volume control.
//!
//! Renders a vertical slider track with a filled region and draggable thumb.
//! Drag interaction adjusts track volume (0.0-1.0). Double-click resets
//! to the default value (0.8). Displays a dB label at the bottom.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::types::track::TrackId;

/// Default volume when double-clicking (linear, ~-1.9 dB).
const DEFAULT_VOLUME: f32 = 0.8;

/// Color constants for the fader.
const TRACK_BG: (u8, u8, u8) = (0x2D, 0x2D, 0x2D);
const FILL_COLOR: (u8, u8, u8) = (0x5B, 0x9B, 0xD5);
const THUMB_COLOR: (u8, u8, u8) = (0x8C, 0xC4, 0xEC);
const LABEL_COLOR: (u8, u8, u8) = (0xB0, 0xB0, 0xB0);

/// Vertical fader with drag interaction and dB readout.
pub struct Fader {
    track_id: TrackId,
    dragging: bool,
    drag_start_y: f32,
    drag_start_value: f32,
}

impl Fader {
    pub fn new(cx: &mut Context, track_id: TrackId) -> Handle<'_, Self> {
        Self {
            track_id,
            dragging: false,
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        }
        .build(cx, |_cx| {})
    }
}

/// Convert a linear volume (0.0-1.0) to decibels for display.
/// Returns `-inf` for 0.0 and `0 dB` for 1.0.
fn volume_to_db(volume: f32) -> f32 {
    if volume <= 0.0 {
        return f32::NEG_INFINITY;
    }
    20.0 * volume.log10()
}

/// Format dB value for display.
fn format_db(db: f32) -> String {
    if db.is_infinite() && db.is_sign_negative() {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", db)
    }
}

impl View for Fader {
    fn element(&self) -> Option<&'static str> {
        Some("fader")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        // Read current volume from model
        let volume = cx
            .data::<AppData>()
            .and_then(|app| app.track(self.track_id))
            .map(|track| track.volume)
            .unwrap_or(DEFAULT_VOLUME);

        let padding = 2.0 * scale;
        let label_area_h = 14.0 * scale;
        let track_x = bounds.x + padding;
        let track_y = bounds.y + padding;
        let track_w = bounds.w - 2.0 * padding;
        let track_h = bounds.h - 2.0 * padding - label_area_h;
        let corner_r = 2.0 * scale;

        // -- Background track (rounded rect) --
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, TRACK_BG.0, TRACK_BG.1, TRACK_BG.2));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);

        let bg_rrect = vg::RRect::new_rect_xy(
            vg::Rect::from_xywh(track_x, track_y, track_w, track_h),
            corner_r,
            corner_r,
        );
        canvas.draw_rrect(bg_rrect, &bg_paint);

        // -- Filled region from bottom --
        let fill_h = (volume.clamp(0.0, 1.0) * track_h).max(0.0);
        let fill_y = track_y + track_h - fill_h;

        if fill_h > 0.0 {
            let mut fill_paint = vg::Paint::default();
            fill_paint.set_color(vg::Color::from_argb(
                255,
                FILL_COLOR.0,
                FILL_COLOR.1,
                FILL_COLOR.2,
            ));
            fill_paint.set_style(vg::PaintStyle::Fill);
            fill_paint.set_anti_alias(true);

            // Clip fill to the track rounded rect bounds
            canvas.save();
            canvas.clip_rrect(bg_rrect, vg::ClipOp::Intersect, true);
            canvas.draw_rect(
                vg::Rect::from_xywh(track_x, fill_y, track_w, fill_h),
                &fill_paint,
            );
            canvas.restore();
        }

        // -- Thumb (horizontal bar at volume position) --
        let thumb_h = 6.0 * scale;
        let thumb_y = fill_y - thumb_h / 2.0;
        let thumb_y = thumb_y.clamp(track_y - thumb_h / 2.0, track_y + track_h - thumb_h / 2.0);

        let mut thumb_paint = vg::Paint::default();
        thumb_paint.set_color(vg::Color::from_argb(
            255,
            THUMB_COLOR.0,
            THUMB_COLOR.1,
            THUMB_COLOR.2,
        ));
        thumb_paint.set_style(vg::PaintStyle::Fill);
        thumb_paint.set_anti_alias(true);

        let thumb_rrect = vg::RRect::new_rect_xy(
            vg::Rect::from_xywh(track_x, thumb_y, track_w, thumb_h),
            1.0 * scale,
            1.0 * scale,
        );
        canvas.draw_rrect(thumb_rrect, &thumb_paint);

        // -- dB label at bottom --
        let db = volume_to_db(volume);
        let label = format_db(db);

        let mut text_paint = vg::Paint::default();
        text_paint.set_color(vg::Color::from_argb(
            255,
            LABEL_COLOR.0,
            LABEL_COLOR.1,
            LABEL_COLOR.2,
        ));
        text_paint.set_anti_alias(true);

        let font = vg::Font::default();
        let label_y = bounds.y + bounds.h - 2.0 * scale;
        let label_x = bounds.x + padding;
        canvas.draw_str(&label, (label_x, label_y), &font, &text_paint);
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                // Read current volume to use as drag baseline
                let volume = cx
                    .data::<AppData>()
                    .and_then(|app| app.track(self.track_id))
                    .map(|track| track.volume)
                    .unwrap_or(DEFAULT_VOLUME);

                self.dragging = true;
                self.drag_start_y = cx.mouse().cursor_y;
                self.drag_start_value = volume;
                cx.capture();
                cx.lock_cursor_icon();
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                // Reset to default volume
                cx.emit(AppEvent::SetTrackVolume {
                    track_id: self.track_id,
                    volume: DEFAULT_VOLUME,
                });
                cx.needs_redraw();
                meta.consume();
            }
            WindowEvent::MouseMove(_, _) => {
                if self.dragging {
                    let bounds = cx.bounds();
                    let cursor_y = cx.mouse().cursor_y;

                    // Dragging up increases volume, down decreases.
                    // Scale so full drag across the widget height = full 0-1 range.
                    let dy = self.drag_start_y - cursor_y;
                    let range_h = bounds.h.max(1.0);
                    let delta = dy / range_h;
                    let new_volume = (self.drag_start_value + delta).clamp(0.0, 1.0);

                    cx.emit(AppEvent::SetTrackVolume {
                        track_id: self.track_id,
                        volume: new_volume,
                    });
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.dragging {
                    self.dragging = false;
                    cx.release();
                    cx.unlock_cursor_icon();
                    meta.consume();
                }
            }
            _ => {}
        });

        // Redraw on poll so the fader reflects external volume changes.
        event.map(|app_event, _meta| {
            if let crate::app_data::AppEvent::PollEngine = app_event {
                cx.needs_redraw();
            }
        });
    }
}
