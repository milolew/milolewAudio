//! Playhead and loop region drawing for track lanes.

use vizia::prelude::*;
use vizia::vg;

/// Draw the playhead line at the given x position.
pub fn draw_playhead(canvas: &Canvas, bounds: BoundingBox, scale: f32, playhead_x: f32) {
    if playhead_x < bounds.x || playhead_x > bounds.x + bounds.w {
        return;
    }

    let mut paint = vg::Paint::default();
    paint.set_color(vg::Color::from_argb(255, 255, 68, 68));
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_stroke_width(1.5 * scale);
    paint.set_anti_alias(true);

    canvas.draw_line(
        (playhead_x, bounds.y),
        (playhead_x, bounds.y + bounds.h),
        &paint,
    );
}

/// Draw a semi-transparent loop region overlay.
pub fn draw_loop_region(canvas: &Canvas, bounds: BoundingBox, loop_start_x: f32, loop_end_x: f32) {
    let lx = loop_start_x.max(bounds.x);
    let rx = loop_end_x.min(bounds.x + bounds.w);

    if rx <= lx {
        return;
    }

    let mut paint = vg::Paint::default();
    paint.set_color(vg::Color::from_argb(20, 255, 200, 40));
    paint.set_style(vg::PaintStyle::Fill);
    paint.set_anti_alias(true);

    canvas.draw_rect(vg::Rect::from_xywh(lx, bounds.y, rx - lx, bounds.h), &paint);
}
