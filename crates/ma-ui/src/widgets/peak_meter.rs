//! Peak meter widget — vertical VU/peak meter with stereo channels.
//!
//! Renders two vertical bars (L/R) with color-coded zones:
//! green (< 0.7), yellow (0.7-0.9), red (> 0.9). Includes a
//! peak-hold indicator line. Redraws every frame via PollEngine timer.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::AppData;
use crate::types::track::TrackId;

/// Stereo peak meter with color-coded level zones and peak hold.
pub struct PeakMeter {
    track_id: TrackId,
}

impl PeakMeter {
    pub fn new(cx: &mut Context, track_id: TrackId) -> Handle<'_, Self> {
        Self { track_id }.build(cx, |_cx| {})
    }
}

/// Color constants for meter zones.
const GREEN: (u8, u8, u8) = (0x4C, 0xAF, 0x50);
const YELLOW: (u8, u8, u8) = (0xFF, 0xC1, 0x07);
const RED: (u8, u8, u8) = (0xF4, 0x43, 0x36);
const BG_COLOR: (u8, u8, u8) = (0x14, 0x14, 0x14);

/// Zone thresholds (fraction of 0.0-1.0 range).
const ZONE_GREEN_END: f32 = 0.7;
const ZONE_YELLOW_END: f32 = 0.9;

/// Draw a single meter channel bar with color zones.
fn draw_channel_bar(
    canvas: &Canvas,
    x: f32,
    bottom: f32,
    bar_width: f32,
    height: f32,
    peak: f32,
    scale: f32,
) {
    let peak = peak.clamp(0.0, 1.0);
    let total_bar_height = peak * height;

    if total_bar_height <= 0.0 {
        return;
    }

    // Green zone: from 0.0 to min(peak, 0.7) of the range
    let green_frac = peak.min(ZONE_GREEN_END);
    let green_h = green_frac * height;
    if green_h > 0.0 {
        let mut paint = vg::Paint::default();
        paint.set_color(vg::Color::from_argb(255, GREEN.0, GREEN.1, GREEN.2));
        paint.set_style(vg::PaintStyle::Fill);
        paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(x, bottom - green_h, bar_width, green_h),
            &paint,
        );
    }

    // Yellow zone: from 0.7 to min(peak, 0.9)
    if peak > ZONE_GREEN_END {
        let yellow_frac = peak.min(ZONE_YELLOW_END) - ZONE_GREEN_END;
        let yellow_h = yellow_frac * height;
        let yellow_top = bottom - ZONE_GREEN_END * height - yellow_h;
        if yellow_h > 0.0 {
            let mut paint = vg::Paint::default();
            paint.set_color(vg::Color::from_argb(255, YELLOW.0, YELLOW.1, YELLOW.2));
            paint.set_style(vg::PaintStyle::Fill);
            paint.set_anti_alias(true);
            canvas.draw_rect(
                vg::Rect::from_xywh(x, yellow_top, bar_width, yellow_h),
                &paint,
            );
        }
    }

    // Red zone: from 0.9 to peak
    if peak > ZONE_YELLOW_END {
        let red_frac = peak - ZONE_YELLOW_END;
        let red_h = red_frac * height;
        let red_top = bottom - total_bar_height;
        if red_h > 0.0 {
            let mut paint = vg::Paint::default();
            paint.set_color(vg::Color::from_argb(255, RED.0, RED.1, RED.2));
            paint.set_style(vg::PaintStyle::Fill);
            paint.set_anti_alias(true);
            canvas.draw_rect(vg::Rect::from_xywh(x, red_top, bar_width, red_h), &paint);
        }
    }

    // Peak hold line (white, 1px)
    if peak > 0.01 {
        let peak_y = bottom - total_bar_height;
        let mut hold_paint = vg::Paint::default();
        hold_paint.set_color(vg::Color::from_argb(255, 255, 255, 255));
        hold_paint.set_style(vg::PaintStyle::Stroke);
        hold_paint.set_stroke_width(1.0 * scale);
        hold_paint.set_anti_alias(true);
        canvas.draw_line((x, peak_y), (x + bar_width, peak_y), &hold_paint);
    }
}

impl View for PeakMeter {
    fn element(&self) -> Option<&'static str> {
        Some("peak-meter")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        // Background
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(
            255, BG_COLOR.0, BG_COLOR.1, BG_COLOR.2,
        ));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg_paint,
        );

        // Read meter peaks from model
        let (peak_l, peak_r) = cx
            .data::<AppData>()
            .map(|app| {
                let peaks = app.mixer.get_meter(self.track_id);
                (peaks.peak_l, peaks.peak_r)
            })
            .unwrap_or((0.0, 0.0));

        // Layout: two bars side by side with a 2px gap in the center
        let gap = 2.0 * scale;
        let padding = 1.0 * scale;
        let usable_w = bounds.w - 2.0 * padding;
        let bar_width = ((usable_w - gap) / 2.0).max(1.0);

        let bar_height = bounds.h - 2.0 * padding;
        let bottom = bounds.y + bounds.h - padding;

        let left_x = bounds.x + padding;
        let right_x = left_x + bar_width + gap;

        // Draw L and R channel bars
        draw_channel_bar(canvas, left_x, bottom, bar_width, bar_height, peak_l, scale);
        draw_channel_bar(
            canvas, right_x, bottom, bar_width, bar_height, peak_r, scale,
        );
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // Continuously redraw when the engine is polled so meters animate.
        event.map(|app_event, _meta| {
            if let crate::app_data::AppEvent::PollEngine = app_event {
                cx.needs_redraw(); // REDRAW: animated — meter levels from engine poll
            }
        });
    }
}
