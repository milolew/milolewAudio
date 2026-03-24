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

mod clip_renderer;
mod grid;
mod live_waveform;
mod playhead;

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::types::time::PPQN;
use crate::types::track::{ClipId, TrackKind};
use crate::widgets::timeline_ruler::TimelineRuler;

use self::clip_renderer::{draw_clip, ClipDrawParams};
use self::grid::{draw_grid, GridParams};
use self::live_waveform::{draw_recording_waveform, WaveformDrawParams};
use self::playhead::{draw_loop_region, draw_playhead};

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
        bg_paint.set_color(vg::Color::from_argb(
            255,
            38 + bg_alpha,
            38 + bg_alpha,
            42 + bg_alpha,
        ));
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
        event.map(|window_event, meta| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                if let Some(app) = cx.data::<AppData>() {
                    if let Some(track) = app.tracks.get(self.track_index) {
                        cx.emit(AppEvent::SelectTrack(track.id));
                    }
                }
                meta.consume();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// TrackLane — custom-drawn clip area with grid, clips, and playhead
// ---------------------------------------------------------------------------

/// Track lane — draws clips, grid lines, playhead, and live waveform for a single track.
struct TrackLane {
    track_index: usize,
    /// Accumulated (tick, peak) samples during recording.
    recording_peaks: Vec<(i64, f32)>,
    /// Whether recording was active on the previous poll.
    was_recording: bool,
}

impl TrackLane {
    fn new(cx: &mut Context, track_index: usize) -> Handle<'_, Self> {
        Self {
            track_index,
            recording_peaks: Vec::new(),
            was_recording: false,
        }
        .build(cx, |_cx| {})
    }

    /// Hit-test: find which clip (if any) is under the given pixel coordinate.
    fn clip_at_position(
        app: &AppData,
        track_index: usize,
        x: f32,
        bounds_x: f32,
    ) -> Option<ClipId> {
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

        let is_selected = arrangement.selected_track == Some(track.id);

        // -- Background (alternating row shading) --
        let base_lum: u8 = if self.track_index.is_multiple_of(2) {
            30
        } else {
            34
        };
        let bg_lum = if is_selected { base_lum + 8 } else { base_lum };

        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, bg_lum, bg_lum, bg_lum + 2));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg_paint,
        );

        // -- Vertical grid lines --
        let ticks_per_beat = PPQN;
        let ticks_per_bar = ticks_per_beat * time_sig.numerator as i64;

        draw_grid(
            canvas,
            &GridParams {
                bounds,
                scale,
                zoom_x,
                scroll_x,
                ticks_per_beat,
                ticks_per_bar,
                time_sig_numerator: time_sig.numerator,
            },
        );

        // -- Loop region overlay --
        if transport.loop_enabled {
            let loop_start_x =
                bounds.x + ((transport.loop_start as f64 - scroll_x) * zoom_x) as f32;
            let loop_end_x = bounds.x + ((transport.loop_end as f64 - scroll_x) * zoom_x) as f32;
            draw_loop_region(canvas, bounds, loop_start_x, loop_end_x);
        }

        // -- Clips --
        let clip_params = ClipDrawParams {
            bounds,
            scale,
            zoom_x,
            scroll_x,
            track_color: track.color,
        };

        for clip in app.clips.iter().filter(|c| c.track_id == track.id) {
            let is_clip_selected = arrangement.selected_clips.contains(&clip.id);
            draw_clip(canvas, clip, &clip_params, is_clip_selected);
        }

        // -- Live recording waveform --
        if !self.recording_peaks.is_empty() {
            draw_recording_waveform(
                canvas,
                &self.recording_peaks,
                &WaveformDrawParams {
                    bounds,
                    scale,
                    zoom_x,
                    scroll_x,
                    track_color: track.color,
                },
            );
        }

        // -- Playhead --
        let playhead_x = bounds.x + ((transport.position as f64 - scroll_x) * zoom_x) as f32;
        draw_playhead(canvas, bounds, scale, playhead_x);

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
        // Accumulate peak meter data during recording for live waveform
        event.map(|app_event, _meta| {
            if let AppEvent::PollEngine = app_event {
                if let Some(app) = cx.data::<AppData>() {
                    let is_recording = app.transport.is_recording;
                    if is_recording {
                        if let Some(track) = app.tracks.get(self.track_index) {
                            let meter = app.mixer.get_meter(track.id);
                            let peak = meter.peak_l.max(meter.peak_r);
                            self.recording_peaks.push((app.transport.position, peak));
                        }
                    } else if self.was_recording {
                        self.recording_peaks.clear();
                    }
                    self.was_recording = is_recording;
                }
                cx.needs_redraw(); // REDRAW: animated — playhead + recording waveform
            }
        });

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

                cx.needs_redraw(); // REDRAW: on-change — zoom/scroll
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
