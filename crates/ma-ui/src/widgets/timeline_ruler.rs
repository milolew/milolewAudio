//! Timeline ruler widget — bar/beat markers with playhead indicator.
//!
//! Draws a horizontal ruler at the top of the arrangement view showing
//! bar numbers, beat lines, and the current playhead position. Clicking
//! on the ruler sets the transport position.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::types::time::PPQN;

/// Timeline ruler — horizontal bar/beat grid with playhead and loop region.
pub struct TimelineRuler;

impl TimelineRuler {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |_cx| {})
    }
}

impl View for TimelineRuler {
    fn element(&self) -> Option<&'static str> {
        Some("timeline-ruler")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        let Some(app) = cx.data::<AppData>() else {
            return;
        };

        let arrangement = &app.arrangement;
        let transport = &app.transport;

        let zoom_x = arrangement.zoom_x;
        let scroll_x = arrangement.scroll_x;
        let time_sig = transport.time_signature;

        // Ticks per beat and per bar
        let ticks_per_beat = PPQN;
        let ticks_per_bar = ticks_per_beat * time_sig.numerator as i64;

        // Visible tick range
        let visible_ticks = if zoom_x > 0.0 {
            (bounds.w as f64 / zoom_x) as i64
        } else {
            0
        };
        let start_tick = scroll_x as i64;
        let end_tick = start_tick + visible_ticks;

        // -- Background --
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, 45, 45, 45));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg_paint,
        );

        // -- Bottom border line --
        let mut border_paint = vg::Paint::default();
        border_paint.set_color(vg::Color::from_argb(255, 60, 60, 60));
        border_paint.set_style(vg::PaintStyle::Stroke);
        border_paint.set_stroke_width(1.0 * scale);
        border_paint.set_anti_alias(true);
        canvas.draw_line(
            (bounds.x, bounds.y + bounds.h - 0.5),
            (bounds.x + bounds.w, bounds.y + bounds.h - 0.5),
            &border_paint,
        );

        // -- Loop region overlay --
        if transport.loop_enabled {
            let loop_x_start =
                bounds.x + ((transport.loop_start as f64 - scroll_x) * zoom_x) as f32;
            let loop_x_end = bounds.x + ((transport.loop_end as f64 - scroll_x) * zoom_x) as f32;

            let lx = loop_x_start.max(bounds.x);
            let rx = loop_x_end.min(bounds.x + bounds.w);

            if rx > lx {
                let mut loop_paint = vg::Paint::default();
                loop_paint.set_color(vg::Color::from_argb(40, 80, 160, 255));
                loop_paint.set_style(vg::PaintStyle::Fill);
                loop_paint.set_anti_alias(true);
                canvas.draw_rect(
                    vg::Rect::from_xywh(lx, bounds.y, rx - lx, bounds.h),
                    &loop_paint,
                );

                // Loop boundary lines
                let mut loop_line_paint = vg::Paint::default();
                loop_line_paint.set_color(vg::Color::from_argb(180, 80, 160, 255));
                loop_line_paint.set_style(vg::PaintStyle::Stroke);
                loop_line_paint.set_stroke_width(1.0 * scale);
                loop_line_paint.set_anti_alias(true);

                if loop_x_start >= bounds.x && loop_x_start <= bounds.x + bounds.w {
                    canvas.draw_line(
                        (loop_x_start, bounds.y),
                        (loop_x_start, bounds.y + bounds.h),
                        &loop_line_paint,
                    );
                }
                if loop_x_end >= bounds.x && loop_x_end <= bounds.x + bounds.w {
                    canvas.draw_line(
                        (loop_x_end, bounds.y),
                        (loop_x_end, bounds.y + bounds.h),
                        &loop_line_paint,
                    );
                }
            }
        }

        // -- Calculate minimum pixel spacing for label skip logic --
        let pixels_per_bar = (ticks_per_bar as f64 * zoom_x) as f32;
        let pixels_per_beat = (ticks_per_beat as f64 * zoom_x) as f32;

        // Determine how many bars to skip between labels to avoid overlap.
        // Minimum label spacing is 60px.
        let min_label_spacing = 60.0 * scale;
        let bar_label_step = if pixels_per_bar > 0.0 {
            ((min_label_spacing / pixels_per_bar).ceil() as i64).max(1)
        } else {
            1
        };

        // Determine whether to show beat lines (only if beats are > 8px apart)
        let show_beats = pixels_per_beat >= 8.0 * scale;

        // -- Bar and beat lines --
        let mut bar_paint = vg::Paint::default();
        bar_paint.set_color(vg::Color::from_argb(255, 128, 128, 128));
        bar_paint.set_style(vg::PaintStyle::Stroke);
        bar_paint.set_stroke_width(1.0 * scale);
        bar_paint.set_anti_alias(true);

        let mut beat_paint = vg::Paint::default();
        beat_paint.set_color(vg::Color::from_argb(255, 80, 80, 80));
        beat_paint.set_style(vg::PaintStyle::Stroke);
        beat_paint.set_stroke_width(0.5 * scale);
        beat_paint.set_anti_alias(true);

        let mut text_paint = vg::Paint::default();
        text_paint.set_color(vg::Color::from_argb(255, 160, 160, 160));
        text_paint.set_anti_alias(true);

        let font = vg::Font::default();

        // First visible bar (snap to bar boundary, go one bar before for partial visibility)
        let first_bar_tick = if start_tick > 0 {
            (start_tick / ticks_per_bar) * ticks_per_bar
        } else {
            0
        };

        let mut bar_tick = first_bar_tick;
        while bar_tick <= end_tick + ticks_per_bar {
            let x = bounds.x + ((bar_tick as f64 - scroll_x) * zoom_x) as f32;

            // Clip to visible bounds
            if x >= bounds.x - 1.0 && x <= bounds.x + bounds.w + 1.0 {
                // Bar line (full height)
                canvas.draw_line(
                    (x, bounds.y + bounds.h * 0.4),
                    (x, bounds.y + bounds.h),
                    &bar_paint,
                );

                // Bar number label
                let bar_num = (bar_tick / ticks_per_bar) + 1;
                if (bar_num - 1) % bar_label_step == 0 {
                    let label = format!("{}", bar_num);
                    canvas.draw_str(
                        &label,
                        (x + 3.0 * scale, bounds.y + bounds.h * 0.35),
                        &font,
                        &text_paint,
                    );
                }
            }

            // Beat lines within this bar
            if show_beats {
                for beat in 1..time_sig.numerator {
                    let beat_tick = bar_tick + ticks_per_beat * beat as i64;
                    let bx = bounds.x + ((beat_tick as f64 - scroll_x) * zoom_x) as f32;

                    if bx >= bounds.x && bx <= bounds.x + bounds.w {
                        canvas.draw_line(
                            (bx, bounds.y + bounds.h * 0.65),
                            (bx, bounds.y + bounds.h),
                            &beat_paint,
                        );
                    }
                }
            }

            bar_tick += ticks_per_bar;
        }

        // -- Playhead --
        let playhead_x = bounds.x + ((transport.position as f64 - scroll_x) * zoom_x) as f32;

        if playhead_x >= bounds.x && playhead_x <= bounds.x + bounds.w {
            let mut playhead_paint = vg::Paint::default();
            playhead_paint.set_color(vg::Color::from_argb(255, 255, 68, 68));
            playhead_paint.set_style(vg::PaintStyle::Stroke);
            playhead_paint.set_stroke_width(2.0 * scale);
            playhead_paint.set_anti_alias(true);

            canvas.draw_line(
                (playhead_x, bounds.y),
                (playhead_x, bounds.y + bounds.h),
                &playhead_paint,
            );

            // Small triangle at the top of the playhead
            let mut tri_paint = vg::Paint::default();
            tri_paint.set_color(vg::Color::from_argb(255, 255, 68, 68));
            tri_paint.set_style(vg::PaintStyle::Fill);
            tri_paint.set_anti_alias(true);

            let tri_size = 5.0 * scale;
            let mut path = vg::Path::new();
            path.move_to((playhead_x - tri_size, bounds.y));
            path.line_to((playhead_x + tri_size, bounds.y));
            path.line_to((playhead_x, bounds.y + tri_size));
            path.close();
            canvas.draw_path(&path, &tri_paint);
        }
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let cursor_x = cx.mouse().cursor_x;

                if let Some(app) = cx.data::<AppData>() {
                    let relative_x = cursor_x - bounds.x;
                    let tick = ((relative_x as f64 / app.arrangement.zoom_x)
                        + app.arrangement.scroll_x) as i64;
                    let tick = tick.max(0);
                    cx.emit(AppEvent::SetPosition(tick));
                }

                meta.consume();
            }
            WindowEvent::MouseScroll(_dx, dy) => {
                // Horizontal scroll on ruler
                if let Some(app) = cx.data::<AppData>() {
                    let scroll_amount = -*dy as f64 * 200.0 / app.arrangement.zoom_x.max(0.001);
                    cx.emit(AppEvent::ScrollArrangementX(scroll_amount));
                }
                cx.needs_redraw();
                meta.consume();
            }
            _ => {}
        });
    }
}
