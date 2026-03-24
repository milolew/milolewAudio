//! Shared meter drawing utilities for peak meters and master strip.
//!
//! Provides color-coded vertical bar rendering with green/yellow/red zones
//! and a peak hold line. Used by both PeakMeter and MasterStrip widgets.

use vizia::prelude::*;
use vizia::vg;

/// Color constants for meter zones.
pub const GREEN: (u8, u8, u8) = (0x4C, 0xAF, 0x50);
pub const YELLOW: (u8, u8, u8) = (0xFF, 0xC1, 0x07);
pub const RED: (u8, u8, u8) = (0xF4, 0x43, 0x36);
pub const METER_BG: (u8, u8, u8) = (0x14, 0x14, 0x14);

/// Zone thresholds (fraction of 0.0–1.0 range).
pub const ZONE_GREEN_END: f32 = 0.7;
pub const ZONE_YELLOW_END: f32 = 0.9;

/// Draw a single vertical meter bar with color-coded zones and peak hold line.
///
/// Zones: green (0.0–0.7), yellow (0.7–0.9), red (0.9–1.0).
/// A white peak hold line is drawn at the top of the filled region.
pub fn draw_meter_bar(
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

    // Green zone: from 0.0 to min(peak, 0.7)
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
