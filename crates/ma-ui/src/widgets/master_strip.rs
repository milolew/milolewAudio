//! Master bus channel strip — shows summed output level.
//!
//! Displays a dedicated "Master" strip at the end of the mixer with
//! stereo peak meters and a dB readout. Reads master peak values
//! from MixerState.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::AppData;
use crate::widgets::meter_utils::{draw_meter_bar, METER_BG};

/// Color constants specific to the master strip.
const LABEL_COLOR: (u8, u8, u8) = (0xB0, 0xB0, 0xB0);
const TITLE_COLOR: (u8, u8, u8) = (0xFF, 0xCC, 0x00);

/// Master bus channel strip for the mixer view.
pub struct MasterStrip;

impl MasterStrip {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |_cx| {})
    }
}

/// Format peak dB into a stack-allocated buffer.
fn format_db(peak: f32, buf: &mut [u8; 16]) -> &str {
    use std::io::Write;
    if peak <= 0.0 {
        "-inf"
    } else {
        let db = 20.0 * peak.log10();
        let mut cursor = std::io::Cursor::new(&mut buf[..]);
        let _ = write!(cursor, "{:.1}", db);
        let len = cursor.position() as usize;
        std::str::from_utf8(&buf[..len]).unwrap_or("?")
    }
}

impl View for MasterStrip {
    fn element(&self) -> Option<&'static str> {
        Some("master-strip")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        let (peak_l, peak_r) = cx
            .data::<AppData>()
            .map(|app| (app.mixer.master_peak_l, app.mixer.master_peak_r))
            .unwrap_or((0.0, 0.0));

        // Background
        let mut bg = vg::Paint::default();
        bg.set_color(vg::Color::from_argb(
            255, METER_BG.0, METER_BG.1, METER_BG.2,
        ));
        bg.set_style(vg::PaintStyle::Fill);
        bg.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg,
        );

        // Title "MASTER"
        let title_h = 20.0 * scale;
        let mut title_paint = vg::Paint::default();
        title_paint.set_color(vg::Color::from_argb(
            255,
            TITLE_COLOR.0,
            TITLE_COLOR.1,
            TITLE_COLOR.2,
        ));
        title_paint.set_anti_alias(true);
        let font = vg::Font::default();
        canvas.draw_str(
            "MASTER",
            (bounds.x + 4.0 * scale, bounds.y + title_h - 4.0 * scale),
            &font,
            &title_paint,
        );

        // Meter area
        let meter_top = bounds.y + title_h + 2.0 * scale;
        let db_label_h = 14.0 * scale;
        let meter_h = (bounds.h - title_h - db_label_h - 6.0 * scale).max(10.0);
        let gap = 3.0 * scale;
        let padding = 4.0 * scale;
        let usable_w = bounds.w - 2.0 * padding;
        let bar_w = ((usable_w - gap) / 2.0).max(3.0);
        let bottom = meter_top + meter_h;

        let left_x = bounds.x + padding;
        let right_x = left_x + bar_w + gap;

        draw_meter_bar(canvas, left_x, bottom, bar_w, meter_h, peak_l, scale);
        draw_meter_bar(canvas, right_x, bottom, bar_w, meter_h, peak_r, scale);

        // dB labels
        let mut label_paint = vg::Paint::default();
        label_paint.set_color(vg::Color::from_argb(
            255,
            LABEL_COLOR.0,
            LABEL_COLOR.1,
            LABEL_COLOR.2,
        ));
        label_paint.set_anti_alias(true);

        let label_y = bounds.y + bounds.h - 2.0 * scale;

        let mut buf_l = [0u8; 16];
        let label_l = format_db(peak_l, &mut buf_l);
        canvas.draw_str(label_l, (left_x, label_y), &font, &label_paint);

        let mut buf_r = [0u8; 16];
        let label_r = format_db(peak_r, &mut buf_r);
        canvas.draw_str(label_r, (right_x, label_y), &font, &label_paint);
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _meta| {
            if let crate::app_data::AppEvent::PollEngine = app_event {
                cx.needs_redraw();
            }
        });
    }
}
