//! Grid drawing — vertical bar and beat lines for track lanes.

use vizia::prelude::*;
use vizia::vg;

use crate::types::time::Tick;

/// Parameters for drawing the bar/beat grid.
pub struct GridParams {
    pub bounds: BoundingBox,
    pub scale: f32,
    pub zoom_x: f64,
    pub scroll_x: f64,
    pub ticks_per_beat: Tick,
    pub ticks_per_bar: Tick,
    pub time_sig_numerator: u8,
}

/// Draw vertical grid lines (bars and beats) across a track lane.
pub fn draw_grid(canvas: &Canvas, params: &GridParams) {
    let bounds = params.bounds;
    let scale = params.scale;
    let zoom_x = params.zoom_x;
    let scroll_x = params.scroll_x;

    let visible_ticks = if zoom_x > 0.0 {
        (bounds.w as f64 / zoom_x) as i64
    } else {
        0
    };
    let start_tick = scroll_x as i64;
    let end_tick = start_tick + visible_ticks;

    let pixels_per_beat = (params.ticks_per_beat as f64 * zoom_x) as f32;
    let show_beats = pixels_per_beat >= 8.0 * scale;

    let mut bar_line_paint = vg::Paint::default();
    bar_line_paint.set_color(vg::Color::from_argb(60, 128, 128, 128));
    bar_line_paint.set_style(vg::PaintStyle::Stroke);
    bar_line_paint.set_stroke_width(0.5 * scale);
    bar_line_paint.set_anti_alias(true);

    let mut beat_line_paint = vg::Paint::default();
    beat_line_paint.set_color(vg::Color::from_argb(30, 128, 128, 128));
    beat_line_paint.set_style(vg::PaintStyle::Stroke);
    beat_line_paint.set_stroke_width(0.5 * scale);
    beat_line_paint.set_anti_alias(true);

    let first_bar_tick = if start_tick > 0 {
        (start_tick / params.ticks_per_bar) * params.ticks_per_bar
    } else {
        0
    };

    let mut bar_tick = first_bar_tick;
    while bar_tick <= end_tick + params.ticks_per_bar {
        let x = bounds.x + ((bar_tick as f64 - scroll_x) * zoom_x) as f32;

        if x >= bounds.x - 1.0 && x <= bounds.x + bounds.w + 1.0 {
            canvas.draw_line((x, bounds.y), (x, bounds.y + bounds.h), &bar_line_paint);
        }

        if show_beats {
            for beat in 1..params.time_sig_numerator {
                let beat_tick = bar_tick + params.ticks_per_beat * beat as i64;
                let bx = bounds.x + ((beat_tick as f64 - scroll_x) * zoom_x) as f32;
                if bx >= bounds.x && bx <= bounds.x + bounds.w {
                    canvas.draw_line((bx, bounds.y), (bx, bounds.y + bounds.h), &beat_line_paint);
                }
            }
        }

        bar_tick += params.ticks_per_bar;
    }
}
