//! Arrangement View — horizontal timeline with track headers, lanes, and clips.
//!
//! Layout:
//! ```text
//! +-------------------+-------------------------------------+
//! | (corner spacer)   |         TimelineRuler               |
//! +-------------------+-------------------------------------+
//! | Track Header 1    |         TrackLane 1 (clips, grid)   |
//! +-------------------+-------------------------------------+
//! | Track Header 2    |         TrackLane 2                  |
//! +-------------------+-------------------------------------+
//! | ...               |         ...                          |
//! +-------------------+-------------------------------------+
//! ```

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::types::time::PPQN;
use crate::types::track::{ClipId, TrackKind};
use crate::widgets::timeline_ruler::TimelineRuler;

/// Width of the track header panel in pixels.
const HEADER_WIDTH: f32 = 180.0;

/// Arrangement view — top-level container for timeline and track lanes.
pub struct ArrangementView;

impl ArrangementView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            // Top row: corner spacer + timeline ruler
            HStack::new(cx, |cx| {
                // Corner spacer — matches track header width
                Element::new(cx)
                    .width(Pixels(HEADER_WIDTH))
                    .height(Stretch(1.0))
                    .class("ruler-corner");

                // Timeline ruler fills remaining width
                TimelineRuler::new(cx)
                    .width(Stretch(1.0))
                    .height(Stretch(1.0));
            })
            .height(Pixels(28.0))
            .width(Stretch(1.0))
            .class("ruler-row");

            // Track rows — scrollable area
            // Re-build when tracks list changes using a Binding on the track count.
            Binding::new(
                cx,
                AppData::tracks.map(|tracks| tracks.len()),
                |cx, track_count| {
                    let count = track_count.get(cx);

                    VStack::new(cx, |cx| {
                        for idx in 0..count {
                            HStack::new(cx, |cx| {
                                // Track header (fixed width)
                                TrackHeader::new(cx, idx)
                                    .width(Pixels(HEADER_WIDTH))
                                    .height(Stretch(1.0));

                                // Track lane (fills remaining width, custom draw)
                                TrackLane::new(cx, idx)
                                    .width(Stretch(1.0))
                                    .height(Stretch(1.0));
                            })
                            .height(Pixels(80.0))
                            .width(Stretch(1.0))
                            .class("track-row");
                        }
                    })
                    .width(Stretch(1.0))
                    .height(Stretch(1.0))
                    .class("track-rows");
                },
            );
        })
    }
}

impl View for ArrangementView {
    fn element(&self) -> Option<&'static str> {
        Some("arrangement-view")
    }
}

// ---------------------------------------------------------------------------
// TrackHeader — left panel showing track name, color, and kind
// ---------------------------------------------------------------------------

/// Track header with color bar, name, and type indicator.
struct TrackHeader {
    track_index: usize,
}

impl TrackHeader {
    fn new(cx: &mut Context, track_index: usize) -> Handle<'_, Self> {
        Self { track_index }.build(cx, |_cx| {})
    }
}

impl View for TrackHeader {
    fn element(&self) -> Option<&'static str> {
        Some("track-header")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        let Some(app) = cx.data::<AppData>() else {
            return;
        };

        let track = match app.tracks.get(self.track_index) {
            Some(t) => t,
            None => return,
        };

        let is_selected = app.arrangement.selected_track == Some(track.id);
        let [r, g, b] = track.color;

        // -- Background --
        let bg_alpha = if is_selected { 50 } else { 30 };
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, 38 + bg_alpha, 38 + bg_alpha, 42 + bg_alpha));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg_paint,
        );

        // -- Color bar on left edge --
        let bar_width = 4.0 * scale;
        let mut color_paint = vg::Paint::default();
        color_paint.set_color(vg::Color::from_argb(255, r, g, b));
        color_paint.set_style(vg::PaintStyle::Fill);
        color_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bar_width, bounds.h),
            &color_paint,
        );

        // -- Selection highlight border --
        if is_selected {
            let mut sel_paint = vg::Paint::default();
            sel_paint.set_color(vg::Color::from_argb(120, r, g, b));
            sel_paint.set_style(vg::PaintStyle::Stroke);
            sel_paint.set_stroke_width(1.5 * scale);
            sel_paint.set_anti_alias(true);
            canvas.draw_rect(
                vg::Rect::from_xywh(
                    bounds.x + 0.5,
                    bounds.y + 0.5,
                    bounds.w - 1.0,
                    bounds.h - 1.0,
                ),
                &sel_paint,
            );
        }

        // -- Track kind label --
        let kind_label = match track.kind {
            TrackKind::Midi => "MIDI",
            TrackKind::Audio => "AUDIO",
        };

        let mut kind_paint = vg::Paint::default();
        kind_paint.set_color(vg::Color::from_argb(140, 180, 180, 180));
        kind_paint.set_anti_alias(true);

        let kind_font = vg::Font::default();
        let text_x = bounds.x + bar_width + 8.0 * scale;
        let kind_y = bounds.y + bounds.h * 0.38;
        canvas.draw_str(kind_label, (text_x, kind_y), &kind_font, &kind_paint);

        // -- Track name --
        let mut name_paint = vg::Paint::default();
        name_paint.set_color(vg::Color::from_argb(255, 220, 220, 220));
        name_paint.set_anti_alias(true);

        let name_font = vg::Font::default();
        let name_y = bounds.y + bounds.h * 0.65;
        canvas.draw_str(&track.name, (text_x, name_y), &name_font, &name_paint);

        // -- Bottom separator line --
        let mut sep_paint = vg::Paint::default();
        sep_paint.set_color(vg::Color::from_argb(255, 50, 50, 50));
        sep_paint.set_style(vg::PaintStyle::Stroke);
        sep_paint.set_stroke_width(0.5 * scale);
        sep_paint.set_anti_alias(true);
        canvas.draw_line(
            (bounds.x, bounds.y + bounds.h - 0.5),
            (bounds.x + bounds.w, bounds.y + bounds.h - 0.5),
            &sep_paint,
        );

        // -- Right separator line --
        canvas.draw_line(
            (bounds.x + bounds.w - 0.5, bounds.y),
            (bounds.x + bounds.w - 0.5, bounds.y + bounds.h),
            &sep_paint,
        );
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                if let Some(app) = cx.data::<AppData>() {
                    if let Some(track) = app.tracks.get(self.track_index) {
                        cx.emit(AppEvent::SelectTrack(track.id));
                    }
                }
                meta.consume();
            }
            _ => {}
        });
    }
}

// ---------------------------------------------------------------------------
// TrackLane — custom-drawn clip area with grid, clips, and playhead
// ---------------------------------------------------------------------------

/// Track lane — draws clips, grid lines, and playhead for a single track.
struct TrackLane {
    track_index: usize,
}

impl TrackLane {
    fn new(cx: &mut Context, track_index: usize) -> Handle<'_, Self> {
        Self { track_index }.build(cx, |_cx| {})
    }

    /// Hit-test: find which clip (if any) is under the given pixel coordinate.
    fn clip_at_position(app: &AppData, track_index: usize, x: f32, bounds_x: f32) -> Option<ClipId> {
        let track = app.tracks.get(track_index)?;
        let arrangement = &app.arrangement;

        for clip in app.clips.iter().filter(|c| c.track_id == track.id) {
            let clip_x = bounds_x
                + ((clip.start_tick as f64 - arrangement.scroll_x) * arrangement.zoom_x) as f32;
            let clip_w = (clip.duration_ticks as f64 * arrangement.zoom_x) as f32;

            if x >= clip_x && x <= clip_x + clip_w {
                return Some(clip.id);
            }
        }
        None
    }
}

impl View for TrackLane {
    fn element(&self) -> Option<&'static str> {
        Some("track-lane")
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

        let track = match app.tracks.get(self.track_index) {
            Some(t) => t,
            None => return,
        };

        let [tr, tg, tb] = track.color;
        let is_selected = arrangement.selected_track == Some(track.id);

        // -- Background (alternating row shading) --
        let base_lum: u8 = if self.track_index % 2 == 0 { 30 } else { 34 };
        let bg_lum = if is_selected {
            base_lum + 8
        } else {
            base_lum
        };

        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, bg_lum, bg_lum, bg_lum + 2));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg_paint,
        );

        // -- Vertical grid lines (bars and beats) --
        let ticks_per_beat = PPQN;
        let ticks_per_bar = ticks_per_beat * time_sig.numerator as i64;

        let visible_ticks = if zoom_x > 0.0 {
            (bounds.w as f64 / zoom_x) as i64
        } else {
            0
        };
        let start_tick = scroll_x as i64;
        let end_tick = start_tick + visible_ticks;

        let pixels_per_beat = (ticks_per_beat as f64 * zoom_x) as f32;
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
            (start_tick / ticks_per_bar) * ticks_per_bar
        } else {
            0
        };

        let mut bar_tick = first_bar_tick;
        while bar_tick <= end_tick + ticks_per_bar {
            let x = bounds.x + ((bar_tick as f64 - scroll_x) * zoom_x) as f32;

            if x >= bounds.x - 1.0 && x <= bounds.x + bounds.w + 1.0 {
                canvas.draw_line(
                    (x, bounds.y),
                    (x, bounds.y + bounds.h),
                    &bar_line_paint,
                );
            }

            if show_beats {
                for beat in 1..time_sig.numerator {
                    let beat_tick = bar_tick + ticks_per_beat * beat as i64;
                    let bx = bounds.x + ((beat_tick as f64 - scroll_x) * zoom_x) as f32;
                    if bx >= bounds.x && bx <= bounds.x + bounds.w {
                        canvas.draw_line(
                            (bx, bounds.y),
                            (bx, bounds.y + bounds.h),
                            &beat_line_paint,
                        );
                    }
                }
            }

            bar_tick += ticks_per_bar;
        }

        // -- Loop region overlay --
        if transport.loop_enabled {
            let loop_x_start =
                bounds.x + ((transport.loop_start as f64 - scroll_x) * zoom_x) as f32;
            let loop_x_end =
                bounds.x + ((transport.loop_end as f64 - scroll_x) * zoom_x) as f32;

            let lx = loop_x_start.max(bounds.x);
            let rx = loop_x_end.min(bounds.x + bounds.w);

            if rx > lx {
                let mut loop_paint = vg::Paint::default();
                loop_paint.set_color(vg::Color::from_argb(15, 80, 160, 255));
                loop_paint.set_style(vg::PaintStyle::Fill);
                loop_paint.set_anti_alias(true);
                canvas.draw_rect(
                    vg::Rect::from_xywh(lx, bounds.y, rx - lx, bounds.h),
                    &loop_paint,
                );
            }
        }

        // -- Clips --
        let clip_padding = 2.0 * scale;
        let clip_corner_radius = 3.0 * scale;
        let clip_font = vg::Font::default();

        for clip in app.clips.iter().filter(|c| c.track_id == track.id) {
            let clip_x =
                bounds.x + ((clip.start_tick as f64 - scroll_x) * zoom_x) as f32;
            let clip_w = (clip.duration_ticks as f64 * zoom_x) as f32;
            let clip_end_x = clip_x + clip_w;

            // Skip clips entirely outside visible area
            if clip_end_x < bounds.x || clip_x > bounds.x + bounds.w {
                continue;
            }

            // Clamp clip rect to visible area for drawing
            let draw_x = clip_x.max(bounds.x);
            let draw_end_x = clip_end_x.min(bounds.x + bounds.w);
            let draw_w = draw_end_x - draw_x;

            if draw_w <= 0.0 {
                continue;
            }

            let clip_y = bounds.y + clip_padding;
            let clip_h = bounds.h - clip_padding * 2.0;

            // Clip fill (semi-transparent track color)
            let mut clip_fill = vg::Paint::default();
            clip_fill.set_color(vg::Color::from_argb(100, tr, tg, tb));
            clip_fill.set_style(vg::PaintStyle::Fill);
            clip_fill.set_anti_alias(true);

            let clip_rect = vg::Rect::from_xywh(draw_x, clip_y, draw_w, clip_h);
            let rrect = vg::RRect::new_rect_xy(clip_rect, clip_corner_radius, clip_corner_radius);
            canvas.draw_rrect(rrect, &clip_fill);

            // Clip border
            let is_clip_selected = arrangement.selected_clips.contains(&clip.id);
            let mut clip_border = vg::Paint::default();
            if is_clip_selected {
                clip_border.set_color(vg::Color::from_argb(220, 255, 255, 255));
                clip_border.set_stroke_width(1.5 * scale);
            } else {
                clip_border.set_color(vg::Color::from_argb(140, tr, tg, tb));
                clip_border.set_stroke_width(1.0 * scale);
            }
            clip_border.set_style(vg::PaintStyle::Stroke);
            clip_border.set_anti_alias(true);
            canvas.draw_rrect(rrect, &clip_border);

            // Clip header bar (top strip with brighter color)
            let header_h = 14.0 * scale;
            if clip_h > header_h + 2.0 {
                let mut header_paint = vg::Paint::default();
                header_paint.set_color(vg::Color::from_argb(160, tr, tg, tb));
                header_paint.set_style(vg::PaintStyle::Fill);
                header_paint.set_anti_alias(true);

                let header_rect =
                    vg::Rect::from_xywh(draw_x, clip_y, draw_w, header_h);
                // Use a clipped rounded rect for the header portion
                canvas.save();
                canvas.clip_rect(header_rect, None, Some(true));
                canvas.draw_rrect(rrect, &header_paint);
                canvas.restore();
            }

            // Clip name text (inside header area)
            if draw_w > 20.0 * scale {
                let mut name_paint = vg::Paint::default();
                name_paint.set_color(vg::Color::from_argb(255, 240, 240, 240));
                name_paint.set_anti_alias(true);

                let text_x = draw_x + 4.0 * scale;
                let text_y = clip_y + header_h - 3.0 * scale;

                canvas.save();
                canvas.clip_rect(
                    vg::Rect::from_xywh(draw_x, clip_y, draw_w, header_h),
                    None,
                    Some(true),
                );
                canvas.draw_str(&clip.name, (text_x, text_y), &clip_font, &name_paint);
                canvas.restore();
            }

            // Mini MIDI note rectangles (for MIDI clips with notes)
            if !clip.notes.is_empty() && clip_h > header_h + 8.0 {
                let note_area_y = clip_y + header_h + 1.0;
                let note_area_h = clip_h - header_h - 2.0;

                // Find pitch range in this clip
                let min_pitch = clip.notes.iter().map(|n| n.pitch).min().unwrap_or(0);
                let max_pitch = clip.notes.iter().map(|n| n.pitch).max().unwrap_or(127);
                let pitch_range = (max_pitch - min_pitch).max(1) as f32 + 2.0;

                let mut note_paint = vg::Paint::default();
                note_paint.set_color(vg::Color::from_argb(200, tr, tg, tb));
                note_paint.set_style(vg::PaintStyle::Fill);
                note_paint.set_anti_alias(true);

                // Clip rendering area for notes
                canvas.save();
                canvas.clip_rect(
                    vg::Rect::from_xywh(draw_x, note_area_y, draw_w, note_area_h),
                    None,
                    Some(true),
                );

                for note in &clip.notes {
                    let note_x = bounds.x
                        + ((note.start_tick as f64 - scroll_x) * zoom_x) as f32;
                    let note_w =
                        (note.duration_ticks as f64 * zoom_x) as f32;

                    // Y position: higher pitches at top
                    let pitch_offset = (max_pitch - note.pitch) as f32 + 1.0;
                    let note_y =
                        note_area_y + (pitch_offset / pitch_range) * note_area_h;
                    let note_h = (note_area_h / pitch_range).max(1.0).min(4.0 * scale);

                    if note_x + note_w >= draw_x && note_x <= draw_end_x {
                        canvas.draw_rect(
                            vg::Rect::from_xywh(
                                note_x.max(draw_x),
                                note_y,
                                note_w.min(draw_end_x - note_x.max(draw_x)),
                                note_h,
                            ),
                            &note_paint,
                        );
                    }
                }

                canvas.restore();
            }
        }

        // -- Playhead --
        let playhead_x =
            bounds.x + ((transport.position as f64 - scroll_x) * zoom_x) as f32;

        if playhead_x >= bounds.x && playhead_x <= bounds.x + bounds.w {
            let mut playhead_paint = vg::Paint::default();
            playhead_paint.set_color(vg::Color::from_argb(255, 255, 68, 68));
            playhead_paint.set_style(vg::PaintStyle::Stroke);
            playhead_paint.set_stroke_width(1.5 * scale);
            playhead_paint.set_anti_alias(true);

            canvas.draw_line(
                (playhead_x, bounds.y),
                (playhead_x, bounds.y + bounds.h),
                &playhead_paint,
            );
        }

        // -- Bottom separator --
        let mut sep_paint = vg::Paint::default();
        sep_paint.set_color(vg::Color::from_argb(255, 45, 45, 45));
        sep_paint.set_style(vg::PaintStyle::Stroke);
        sep_paint.set_stroke_width(0.5 * scale);
        sep_paint.set_anti_alias(true);
        canvas.draw_line(
            (bounds.x, bounds.y + bounds.h - 0.5),
            (bounds.x + bounds.w, bounds.y + bounds.h - 0.5),
            &sep_paint,
        );
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseScroll(dx, dy) => {
                let modifiers = cx.modifiers();

                if modifiers.contains(Modifiers::CTRL) {
                    // Ctrl + scroll = zoom
                    let factor = if *dy > 0.0 { 1.1 } else { 0.9 };
                    cx.emit(AppEvent::ZoomArrangement(factor));
                } else {
                    // Regular scroll: vertical with dy, horizontal with dx or shift+dy
                    if modifiers.contains(Modifiers::SHIFT) {
                        // Shift + scroll = horizontal scroll
                        if let Some(app) = cx.data::<AppData>() {
                            let scroll_amount =
                                -*dy as f64 * 200.0 / app.arrangement.zoom_x.max(0.001);
                            cx.emit(AppEvent::ScrollArrangementX(scroll_amount));
                        }
                    } else {
                        // Horizontal scroll from dx
                        if *dx != 0.0 {
                            if let Some(app) = cx.data::<AppData>() {
                                let scroll_amount =
                                    *dx as f64 * 200.0 / app.arrangement.zoom_x.max(0.001);
                                cx.emit(AppEvent::ScrollArrangementX(scroll_amount));
                            }
                        }
                        // Vertical scroll from dy
                        if *dy != 0.0 {
                            cx.emit(AppEvent::ScrollArrangementY(-*dy * 20.0));
                        }
                    }
                }

                cx.needs_redraw();
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                // Double-click on a clip opens the piano roll
                let cursor_x = cx.mouse().cursor_x;

                if let Some(app) = cx.data::<AppData>() {
                    if let Some(clip_id) =
                        Self::clip_at_position(app, self.track_index, cursor_x, cx.bounds().x)
                    {
                        cx.emit(AppEvent::OpenPianoRoll(clip_id));
                    }
                }

                meta.consume();
            }
            WindowEvent::MouseDown(MouseButton::Left) => {
                // Single click selects the track
                if let Some(app) = cx.data::<AppData>() {
                    if let Some(track) = app.tracks.get(self.track_index) {
                        cx.emit(AppEvent::SelectTrack(track.id));
                    }
                }
                meta.consume();
            }
            _ => {}
        });
    }
}
