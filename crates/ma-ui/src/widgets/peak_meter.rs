//! Peak meter widget — vertical VU/peak meter with stereo channels.
//!
//! Renders two vertical bars (L/R) with color-coded zones:
//! green (< 0.7), yellow (0.7-0.9), red (> 0.9). Includes a
//! peak-hold indicator line. Redraws every frame via PollEngine timer.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::AppData;
use crate::types::track::TrackId;
use crate::widgets::meter_utils::{draw_meter_bar, METER_BG};

/// Stereo peak meter with color-coded level zones and peak hold.
pub struct PeakMeter {
    track_id: TrackId,
}

impl PeakMeter {
    pub fn new(cx: &mut Context, track_id: TrackId) -> Handle<'_, Self> {
        Self { track_id }.build(cx, |_cx| {})
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
            255, METER_BG.0, METER_BG.1, METER_BG.2,
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
        draw_meter_bar(canvas, left_x, bottom, bar_width, bar_height, peak_l, scale);
        draw_meter_bar(
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
