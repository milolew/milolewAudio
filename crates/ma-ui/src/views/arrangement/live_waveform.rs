//! Live waveform rendering — growing waveform display during recording.
//!
//! Draws accumulated peak samples as a symmetric waveform centered in the track lane.
//! Each sample is a (tick, amplitude) pair captured per poll frame during recording.

use vizia::prelude::*;
use vizia::vg;

/// Parameters for drawing the live recording waveform.
pub struct WaveformDrawParams {
    pub bounds: BoundingBox,
    pub scale: f32,
    pub zoom_x: f64,
    pub scroll_x: f64,
    pub track_color: [u8; 3],
}

/// Draw a growing waveform from accumulated (tick, peak) samples.
///
/// Each entry in `peaks` is `(tick_position, peak_amplitude_0_to_1)`,
/// captured once per poll frame while recording is active.
pub fn draw_recording_waveform(
    canvas: &Canvas,
    peaks: &[(i64, f32)],
    params: &WaveformDrawParams,
) {
    if peaks.is_empty() {
        return;
    }

    let bounds = params.bounds;
    let [tr, tg, tb] = params.track_color;

    let padding = 2.0 * params.scale;
    let waveform_y = bounds.y + padding;
    let waveform_h = bounds.h - padding * 2.0;
    let center_y = waveform_y + waveform_h / 2.0;

    let mut paint = vg::Paint::default();
    paint.set_color(vg::Color::from_argb(160, tr, tg, tb));
    paint.set_style(vg::PaintStyle::Fill);
    paint.set_anti_alias(true);

    canvas.save();
    canvas.clip_rect(
        vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
        None,
        Some(true),
    );

    for (i, &(tick, peak)) in peaks.iter().enumerate() {
        let x = bounds.x + ((tick as f64 - params.scroll_x) * params.zoom_x) as f32;

        // Compute bar width from distance to next sample (or a default)
        let next_x = peaks.get(i + 1).map_or(x + 2.0 * params.scale, |&(nt, _)| {
            bounds.x + ((nt as f64 - params.scroll_x) * params.zoom_x) as f32
        });
        let bar_w = (next_x - x).clamp(1.0, 4.0 * params.scale);

        // Skip if outside visible area
        if x + bar_w < bounds.x || x > bounds.x + bounds.w {
            continue;
        }

        let amplitude = peak.clamp(0.0, 1.0);
        let bar_h = amplitude * (waveform_h / 2.0);
        if bar_h < 0.5 {
            continue;
        }

        // Symmetric bar around center line
        canvas.draw_rect(
            vg::Rect::from_xywh(x, center_y - bar_h, bar_w, bar_h * 2.0),
            &paint,
        );
    }

    canvas.restore();
}
