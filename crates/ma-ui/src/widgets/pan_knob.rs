//! Pan knob widget — rotary control for stereo pan position.
//!
//! Renders a circular arc indicator showing the pan position from
//! full left (-1.0) through center (0.0) to full right (1.0).
//! Horizontal drag adjusts pan. Double-click resets to center.

use std::f32::consts::PI;

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::types::track::TrackId;

/// Color constants for the knob.
const KNOB_BG: (u8, u8, u8) = (0x2D, 0x2D, 0x2D);
const KNOB_ARC: (u8, u8, u8) = (0x5B, 0x9B, 0xD5);
const KNOB_INDICATOR: (u8, u8, u8) = (0x8C, 0xC4, 0xEC);
const LABEL_COLOR: (u8, u8, u8) = (0xB0, 0xB0, 0xB0);
const CENTER_TICK: (u8, u8, u8) = (0x60, 0x60, 0x60);

/// Start angle (7 o'clock position) and sweep range.
const ARC_START: f32 = 0.75 * PI; // 135 degrees from 3 o'clock
const ARC_RANGE: f32 = 1.5 * PI; // 270 degree sweep

/// Rotary pan knob with drag interaction.
pub struct PanKnob {
    track_id: TrackId,
    dragging: bool,
    drag_start_x: f32,
    drag_start_value: f32,
}

impl PanKnob {
    pub fn new(cx: &mut Context, track_id: TrackId) -> Handle<'_, Self> {
        Self {
            track_id,
            dragging: false,
            drag_start_x: 0.0,
            drag_start_value: 0.0,
        }
        .build(cx, |_cx| {})
    }
}

/// Format pan value for display.
fn format_pan(pan: f32, buf: &mut [u8; 8]) -> &str {
    use std::io::Write;
    if pan.abs() < 0.01 {
        "C"
    } else {
        let mut cursor = std::io::Cursor::new(&mut buf[..]);
        if pan < 0.0 {
            let _ = write!(cursor, "L{:.0}", -pan * 100.0);
        } else {
            let _ = write!(cursor, "R{:.0}", pan * 100.0);
        }
        let len = cursor.position() as usize;
        std::str::from_utf8(&buf[..len]).unwrap_or("?")
    }
}

impl View for PanKnob {
    fn element(&self) -> Option<&'static str> {
        Some("pan-knob")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        let pan = cx
            .data::<AppData>()
            .and_then(|app| app.track(self.track_id))
            .map(|t| t.pan)
            .unwrap_or(0.0);

        let label_h = 12.0 * scale;
        let knob_size = (bounds.w.min(bounds.h - label_h) - 4.0 * scale).max(8.0);
        let radius = knob_size / 2.0;
        let center_x = bounds.x + bounds.w / 2.0;
        let center_y = bounds.y + (bounds.h - label_h) / 2.0;
        let stroke_width = 2.5 * scale;

        // Background circle
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, KNOB_BG.0, KNOB_BG.1, KNOB_BG.2));
        bg_paint.set_style(vg::PaintStyle::Stroke);
        bg_paint.set_stroke_width(stroke_width);
        bg_paint.set_anti_alias(true);
        canvas.draw_circle((center_x, center_y), radius - stroke_width, &bg_paint);

        // Center tick (12 o'clock)
        let mut tick_paint = vg::Paint::default();
        tick_paint.set_color(vg::Color::from_argb(
            255,
            CENTER_TICK.0,
            CENTER_TICK.1,
            CENTER_TICK.2,
        ));
        tick_paint.set_style(vg::PaintStyle::Stroke);
        tick_paint.set_stroke_width(1.0 * scale);
        tick_paint.set_anti_alias(true);
        let tick_outer = radius - stroke_width * 0.5;
        let tick_inner = tick_outer - 4.0 * scale;
        let top_angle = -PI / 2.0;
        canvas.draw_line(
            (
                center_x + tick_inner * top_angle.cos(),
                center_y + tick_inner * top_angle.sin(),
            ),
            (
                center_x + tick_outer * top_angle.cos(),
                center_y + tick_outer * top_angle.sin(),
            ),
            &tick_paint,
        );

        // Active arc from center to current pan position
        // Pan -1.0 → start of arc, 0.0 → middle, 1.0 → end of arc
        let normalized = (pan + 1.0) / 2.0; // 0.0 to 1.0
        let center_norm = 0.5;
        let arc_radius = radius - stroke_width;

        if (normalized - center_norm).abs() > 0.005 {
            let mut arc_paint = vg::Paint::default();
            arc_paint.set_color(vg::Color::from_argb(
                255, KNOB_ARC.0, KNOB_ARC.1, KNOB_ARC.2,
            ));
            arc_paint.set_style(vg::PaintStyle::Stroke);
            arc_paint.set_stroke_width(stroke_width);
            arc_paint.set_anti_alias(true);
            arc_paint.set_stroke_cap(vg::PaintCap::Round);

            let (sweep_start, sweep_end) = if normalized < center_norm {
                (
                    ARC_START + normalized * ARC_RANGE,
                    ARC_START + center_norm * ARC_RANGE,
                )
            } else {
                (
                    ARC_START + center_norm * ARC_RANGE,
                    ARC_START + normalized * ARC_RANGE,
                )
            };

            let rect = vg::Rect::from_xywh(
                center_x - arc_radius,
                center_y - arc_radius,
                arc_radius * 2.0,
                arc_radius * 2.0,
            );
            let mut path = vg::Path::new();
            path.add_arc(
                rect,
                sweep_start.to_degrees(),
                (sweep_end - sweep_start).to_degrees(),
            );
            canvas.draw_path(&path, &arc_paint);
        }

        // Indicator dot at current position
        let angle = ARC_START + normalized * ARC_RANGE;
        let dot_r = 3.0 * scale;
        let dot_x = center_x + arc_radius * angle.cos();
        let dot_y = center_y + arc_radius * angle.sin();

        let mut dot_paint = vg::Paint::default();
        dot_paint.set_color(vg::Color::from_argb(
            255,
            KNOB_INDICATOR.0,
            KNOB_INDICATOR.1,
            KNOB_INDICATOR.2,
        ));
        dot_paint.set_style(vg::PaintStyle::Fill);
        dot_paint.set_anti_alias(true);
        canvas.draw_circle((dot_x, dot_y), dot_r, &dot_paint);

        // Label below
        let mut text_paint = vg::Paint::default();
        text_paint.set_color(vg::Color::from_argb(
            255,
            LABEL_COLOR.0,
            LABEL_COLOR.1,
            LABEL_COLOR.2,
        ));
        text_paint.set_anti_alias(true);

        let font = vg::Font::default();
        let mut pan_buf = [0u8; 8];
        let label = format_pan(pan, &mut pan_buf);
        let label_y = bounds.y + bounds.h - 2.0 * scale;
        canvas.draw_str(label, (bounds.x + 4.0 * scale, label_y), &font, &text_paint);
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let pan = cx
                    .data::<AppData>()
                    .and_then(|app| app.track(self.track_id))
                    .map(|t| t.pan)
                    .unwrap_or(0.0);

                self.dragging = true;
                self.drag_start_x = cx.mouse().cursor_x;
                self.drag_start_value = pan;
                cx.capture();
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                self.dragging = false;
                cx.emit(AppEvent::SetTrackPan {
                    track_id: self.track_id,
                    pan: 0.0,
                });
                cx.needs_redraw();
                meta.consume();
            }
            WindowEvent::MouseMove(_, _) => {
                if self.dragging {
                    let bounds = cx.bounds();
                    let cursor_x = cx.mouse().cursor_x;

                    // Horizontal drag: right = pan right, left = pan left
                    let dx = cursor_x - self.drag_start_x;
                    let range = bounds.w.max(1.0) * 2.0;
                    let delta = dx / range;
                    let new_pan = (self.drag_start_value + delta).clamp(-1.0, 1.0);

                    cx.emit(AppEvent::SetTrackPan {
                        track_id: self.track_id,
                        pan: new_pan,
                    });
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.dragging {
                    self.dragging = false;
                    cx.release();
                    meta.consume();
                }
            }
            _ => {}
        });

        event.map(|app_event, _meta| {
            if let crate::app_data::AppEvent::PollEngine = app_event {
                cx.needs_redraw();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_pan_center() {
        let mut buf = [0u8; 8];
        assert_eq!(format_pan(0.0, &mut buf), "C");
        assert_eq!(format_pan(0.005, &mut buf), "C");
        assert_eq!(format_pan(-0.005, &mut buf), "C");
    }

    #[test]
    fn format_pan_left() {
        let mut buf = [0u8; 8];
        assert_eq!(format_pan(-1.0, &mut buf), "L100");
        assert_eq!(format_pan(-0.5, &mut buf), "L50");
    }

    #[test]
    fn format_pan_right() {
        let mut buf = [0u8; 8];
        assert_eq!(format_pan(1.0, &mut buf), "R100");
        assert_eq!(format_pan(0.5, &mut buf), "R50");
    }
}
