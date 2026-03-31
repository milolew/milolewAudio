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

pub mod clip_interaction;
mod clip_renderer;
pub mod clipboard;
mod grid;
mod live_waveform;
mod playhead;
pub mod selection;
pub mod snap;

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::state::arrangement_state::{ClipSelection, TrackDragState};
use crate::types::time::PPQN;
#[cfg(test)]
use crate::types::track::ClipId;
use crate::types::track::{ClipState, TrackKind};
use crate::widgets::timeline_ruler::TimelineRuler;

use self::clip_interaction::{hit_test_clip_zone, ClipHitZone, ClipInteraction, DRAG_THRESHOLD};
use self::clip_renderer::{draw_clip, draw_ghost_clip, ClipDrawParams};
use self::grid::{draw_grid, GridParams};
use self::live_waveform::{draw_recording_waveform, WaveformDrawParams};
use self::playhead::{draw_loop_region, draw_playhead};
use self::selection::{clips_in_rect, SelectionRect};

/// Width of the track header panel in pixels.
const HEADER_WIDTH: f32 = 180.0;

/// Y offset of the first track row (ruler height).
const RULER_HEIGHT: f32 = 28.0;

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
            .height(Pixels(RULER_HEIGHT))
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

            // Color picker popup overlay
            ColorPickerOverlay::new(cx);
        })
    }
}

/// 16-color palette for track color picking.
const TRACK_COLORS: [[u8; 3]; 16] = [
    [100, 160, 255],
    [255, 140, 80],
    [80, 220, 120],
    [200, 100, 255],
    [255, 200, 60],
    [60, 200, 200],
    [255, 100, 140],
    [160, 220, 80],
    [200, 160, 120],
    [140, 140, 220],
    [220, 180, 220],
    [180, 220, 180],
    [100, 100, 100],
    [200, 200, 200],
    [255, 80, 80],
    [80, 160, 200],
];

/// Color picker overlay — 4x4 grid of color swatches, shown on right-click of track header.
struct ColorPickerOverlay;

impl ColorPickerOverlay {
    fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |_cx| {})
    }
}

impl View for ColorPickerOverlay {
    fn element(&self) -> Option<&'static str> {
        Some("color-picker-overlay")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let Some(app) = cx.data::<AppData>() else {
            return;
        };
        if !app.color_picker.visible {
            return;
        }

        let scale = cx.scale_factor();
        let swatch_size = 20.0 * scale;
        let padding = 3.0 * scale;
        let cols = 4u32;
        let rows = 4u32;
        let grid_w = cols as f32 * (swatch_size + padding) + padding;
        let grid_h = rows as f32 * (swatch_size + padding) + padding;

        // Position below track header, left-aligned with header
        let popup_x = 4.0 * scale;
        let popup_y = app.color_picker.anchor_y;

        // Background
        let mut bg = vg::Paint::default();
        bg.set_color(vg::Color::from_argb(240, 30, 30, 34));
        bg.set_style(vg::PaintStyle::Fill);
        bg.set_anti_alias(true);
        let rect = vg::Rect::from_xywh(popup_x, popup_y, grid_w, grid_h);
        let rrect = vg::RRect::new_rect_xy(rect, 4.0 * scale, 4.0 * scale);
        canvas.draw_rrect(rrect, &bg);

        // Border
        let mut border = vg::Paint::default();
        border.set_color(vg::Color::from_argb(200, 80, 80, 80));
        border.set_style(vg::PaintStyle::Stroke);
        border.set_stroke_width(1.0 * scale);
        border.set_anti_alias(true);
        canvas.draw_rrect(rrect, &border);

        // Color swatches
        for (i, color) in TRACK_COLORS.iter().enumerate() {
            let col = (i % cols as usize) as f32;
            let row = (i / cols as usize) as f32;
            let x = popup_x + padding + col * (swatch_size + padding);
            let y = popup_y + padding + row * (swatch_size + padding);

            let mut paint = vg::Paint::default();
            paint.set_color(vg::Color::from_argb(255, color[0], color[1], color[2]));
            paint.set_style(vg::PaintStyle::Fill);
            paint.set_anti_alias(true);
            let swatch_rect = vg::Rect::from_xywh(x, y, swatch_size, swatch_size);
            let swatch_rrect = vg::RRect::new_rect_xy(swatch_rect, 3.0 * scale, 3.0 * scale);
            canvas.draw_rrect(swatch_rrect, &paint);
        }
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let Some(app) = cx.data::<AppData>() else {
                    return;
                };
                if !app.color_picker.visible {
                    return;
                }
                let Some(track_id) = app.color_picker.track_id else {
                    return;
                };

                let scale = cx.scale_factor();
                let swatch_size = 20.0 * scale;
                let padding = 3.0 * scale;
                let cols = 4usize;
                let popup_x = 4.0 * scale;
                let popup_y = app.color_picker.anchor_y;

                let mx = cx.mouse().cursor_x;
                let my = cx.mouse().cursor_y;

                // Check if click is inside a swatch
                for (i, color) in TRACK_COLORS.iter().enumerate() {
                    let col = (i % cols) as f32;
                    let row = (i / cols) as f32;
                    let sx = popup_x + padding + col * (swatch_size + padding);
                    let sy = popup_y + padding + row * (swatch_size + padding);

                    if mx >= sx && mx <= sx + swatch_size && my >= sy && my <= sy + swatch_size {
                        cx.emit(AppEvent::SetTrackColor {
                            track_id,
                            color: *color,
                        });
                        meta.consume();
                        return;
                    }
                }

                // Click outside swatches — dismiss
                cx.emit(AppEvent::HideColorPicker);
                meta.consume();
            }
        });
    }
}

impl View for ArrangementView {
    fn element(&self) -> Option<&'static str> {
        Some("arrangement-view")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            if let WindowEvent::KeyDown(code, _) = window_event {
                let modifiers = cx.modifiers();
                match code {
                    Code::KeyE if modifiers.contains(Modifiers::CTRL) => {
                        cx.emit(AppEvent::SplitClipAtPlayhead);
                        meta.consume();
                    }
                    Code::KeyD if modifiers.contains(Modifiers::CTRL) => {
                        cx.emit(AppEvent::DuplicateSelectedClips);
                        meta.consume();
                    }
                    Code::KeyC if modifiers.contains(Modifiers::CTRL) => {
                        cx.emit(AppEvent::CopySelectedClips);
                        meta.consume();
                    }
                    Code::KeyV if modifiers.contains(Modifiers::CTRL) => {
                        cx.emit(AppEvent::PasteClips);
                        meta.consume();
                    }
                    Code::Delete | Code::Backspace => {
                        cx.emit(AppEvent::DeleteSelectedClips);
                        meta.consume();
                    }
                    Code::KeyL => {
                        cx.emit(AppEvent::ToggleLoop);
                        meta.consume();
                    }
                    Code::Equal | Code::NumpadAdd => {
                        cx.emit(AppEvent::ZoomArrangement(1.2));
                        meta.consume();
                    }
                    Code::Minus | Code::NumpadSubtract => {
                        cx.emit(AppEvent::ZoomArrangement(1.0 / 1.2));
                        meta.consume();
                    }
                    _ => {}
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// TrackHeader — left panel showing track name, color, and kind
// ---------------------------------------------------------------------------

/// Track header with color bar, name, and type indicator.
struct TrackHeader {
    track_index: usize,
    /// Pending drag start Y for drag-to-reorder.
    drag_start_y: Option<f32>,
}

impl TrackHeader {
    fn new(cx: &mut Context, track_index: usize) -> Handle<'_, Self> {
        Self {
            track_index,
            drag_start_y: None,
        }
        .build(cx, |_cx| {})
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

        // -- Record arm indicator (red circle, top-right) --
        let arm_radius = 6.0 * scale;
        let arm_cx = bounds.x + bounds.w - 16.0 * scale;
        let arm_cy = bounds.y + 14.0 * scale;
        let mut arm_paint = vg::Paint::default();
        arm_paint.set_anti_alias(true);

        if track.record_armed {
            arm_paint.set_color(vg::Color::from_argb(255, 220, 50, 50));
            arm_paint.set_style(vg::PaintStyle::Fill);
        } else {
            arm_paint.set_color(vg::Color::from_argb(120, 180, 60, 60));
            arm_paint.set_style(vg::PaintStyle::Stroke);
            arm_paint.set_stroke_width(1.5 * scale);
        }
        canvas.draw_circle((arm_cx, arm_cy), arm_radius, &arm_paint);

        // -- Input monitoring indicator ("M" below arm circle) --
        let mon_x = arm_cx - 5.0 * scale;
        let mon_y = arm_cy + 16.0 * scale;
        let mut mon_paint = vg::Paint::default();
        mon_paint.set_anti_alias(true);
        if track.input_monitoring {
            mon_paint.set_color(vg::Color::from_argb(255, 80, 200, 120));
        } else {
            mon_paint.set_color(vg::Color::from_argb(80, 120, 120, 120));
        }
        let mon_font = vg::Font::default();
        canvas.draw_str("M", (mon_x, mon_y), &mon_font, &mon_paint);

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

        // -- Drag reorder indicator --
        if let TrackDragState::Dragging {
            track_index: drag_idx,
            current_y,
            ..
        } = &app.arrangement.track_drag
        {
            if *drag_idx == self.track_index {
                // Highlight the source track being dragged
                let mut drag_paint = vg::Paint::default();
                drag_paint.set_color(vg::Color::from_argb(60, 255, 255, 255));
                drag_paint.set_style(vg::PaintStyle::Fill);
                canvas.draw_rect(
                    vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
                    &drag_paint,
                );

                // Draw drop indicator line at target position
                let track_height = app.arrangement.track_height;
                let relative_y = *current_y - RULER_HEIGHT;
                let target_idx = (relative_y / track_height).round().max(0.0) as usize;
                let indicator_y = RULER_HEIGHT + target_idx as f32 * track_height;

                let mut line_paint = vg::Paint::default();
                line_paint.set_color(vg::Color::from_argb(255, 100, 180, 255));
                line_paint.set_style(vg::PaintStyle::Stroke);
                line_paint.set_stroke_width(2.0 * scale);
                canvas.draw_line(
                    (0.0, indicator_y),
                    (bounds.x + bounds.w, indicator_y),
                    &line_paint,
                );
            }
        }
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            match window_event {
                WindowEvent::MouseDown(MouseButton::Left) => {
                    if let Some(app) = cx.data::<AppData>() {
                        if let Some(track) = app.tracks.get(self.track_index) {
                            let bounds = cx.bounds();
                            let scale = cx.scale_factor();
                            let cursor_x = cx.mouse().cursor_x;
                            let cursor_y = cx.mouse().cursor_y;

                            // Check if click is in arm button area (top-right circle)
                            let arm_cx = bounds.x + bounds.w - 16.0 * scale;
                            let arm_cy = bounds.y + 14.0 * scale;
                            let dx = cursor_x - arm_cx;
                            let dy = cursor_y - arm_cy;
                            let hit_arm = (dx * dx + dy * dy) < (12.0 * scale * 12.0 * scale);

                            // Check if click is in monitoring area ("M" below arm)
                            let mon_cy = arm_cy + 12.0 * scale;
                            let dm_x = cursor_x - arm_cx;
                            let dm_y = cursor_y - mon_cy;
                            let hit_mon = dm_x.abs() < 10.0 * scale && dm_y.abs() < 8.0 * scale;

                            if hit_arm {
                                cx.emit(AppEvent::ToggleRecordArm(track.id));
                            } else if hit_mon {
                                cx.emit(AppEvent::ToggleInputMonitoring(track.id));
                            } else {
                                cx.emit(AppEvent::SelectTrack(track.id));
                                // Start potential drag for reorder
                                self.drag_start_y = Some(cursor_y);
                                cx.capture();
                            }
                        }
                    }
                    cx.needs_redraw();
                    meta.consume();
                }

                WindowEvent::MouseMove(_, _) => {
                    if let Some(start_y) = self.drag_start_y {
                        let cursor_y = cx.mouse().cursor_y;
                        let delta = (cursor_y - start_y).abs();
                        if delta > DRAG_THRESHOLD {
                            cx.emit(AppEvent::UpdateTrackDrag(TrackDragState::Dragging {
                                track_index: self.track_index,
                                start_y,
                                current_y: cursor_y,
                            }));
                            cx.needs_redraw();
                        }
                    }
                }

                WindowEvent::MouseUp(MouseButton::Left) => {
                    if self.drag_start_y.take().is_some() {
                        if let Some(app) = cx.data::<AppData>() {
                            if let TrackDragState::Dragging {
                                track_index,
                                current_y,
                                ..
                            } = &app.arrangement.track_drag
                            {
                                let track_count = app.tracks.len();
                                let track_height = app.arrangement.track_height;
                                // Compute target index from cursor Y relative to first track
                                let first_track_y = RULER_HEIGHT;
                                let relative_y = *current_y - first_track_y;
                                let target = (relative_y / track_height)
                                    .round()
                                    .max(0.0)
                                    .min((track_count - 1) as f32)
                                    as usize;
                                if target != *track_index {
                                    cx.emit(AppEvent::ReorderTrack {
                                        from_index: *track_index,
                                        to_index: target,
                                    });
                                }
                            }
                        }
                        cx.emit(AppEvent::UpdateTrackDrag(TrackDragState::Idle));
                        cx.release();
                        cx.needs_redraw();
                    }
                }

                // Right-click → show color picker popup
                WindowEvent::MouseDown(MouseButton::Right) => {
                    if let Some(app) = cx.data::<AppData>() {
                        if let Some(track) = app.tracks.get(self.track_index) {
                            let bounds = cx.bounds();
                            cx.emit(AppEvent::ShowColorPicker {
                                track_id: track.id,
                                anchor_y: bounds.y + bounds.h,
                            });
                        }
                    }
                    meta.consume();
                }

                _ => {}
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

    /// Gather clips for this lane's track.
    fn track_clips(app: &AppData, track_index: usize) -> Vec<ClipState> {
        let Some(track) = app.tracks.get(track_index) else {
            return Vec::new();
        };
        app.clips
            .iter()
            .filter(|c| c.track_id == track.id)
            .cloned()
            .collect()
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
            let peak_cache = app.audio_peaks.get(&clip.id).map(|arc| arc.as_ref());
            draw_clip(canvas, clip, &clip_params, is_clip_selected, peak_cache);
        }

        // -- Ghost clips during drag --
        if let ClipInteraction::MovingClips { delta_tick, .. } = &arrangement.interaction {
            if *delta_tick != 0 {
                for clip in app.clips.iter().filter(|c| {
                    arrangement.selected_clips.contains(&c.id) && c.track_id == track.id
                }) {
                    let peak_cache = app.audio_peaks.get(&clip.id).map(|arc| arc.as_ref());
                    draw_ghost_clip(canvas, clip, &clip_params, *delta_tick, peak_cache);
                }
            }
        }

        // -- Resize preview --
        if let ClipInteraction::ResizingClip {
            clip_id,
            original_start,
            original_duration,
            ..
        } = &arrangement.interaction
        {
            if let Some(clip) = app
                .clips
                .iter()
                .find(|c| c.id == *clip_id && c.track_id == track.id)
            {
                let resized = ClipState {
                    start_tick: *original_start,
                    duration_ticks: *original_duration,
                    ..clip.clone()
                };
                let peak_cache = app.audio_peaks.get(clip_id).map(|arc| arc.as_ref());
                draw_clip(canvas, &resized, &clip_params, true, peak_cache);
            }
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

        // -- Rubber band selection rectangle (draw on this lane if active) --
        if let ClipInteraction::RubberBand {
            origin_x,
            origin_y,
            current_x,
            current_y,
        } = &arrangement.interaction
        {
            draw_rubber_band(canvas, *origin_x, *origin_y, *current_x, *current_y, scale);
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
                            if track.record_armed {
                                let meter = app.mixer.get_meter(track.id);
                                let peak = meter.peak_l.max(meter.peak_r);
                                if self.recording_peaks.len() < 108_000 {
                                    self.recording_peaks.push((app.transport.position, peak));
                                }
                            }
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
                    // Ctrl + scroll = zoom centered on cursor
                    let factor = if *dy > 0.0 { 1.1 } else { 0.9 };
                    if let Some(app) = cx.data::<AppData>() {
                        let bounds = cx.bounds();
                        let cursor_tick = app.arrangement.x_to_tick(cx.mouse().cursor_x - bounds.x);
                        cx.emit(AppEvent::ZoomArrangementAt {
                            factor,
                            cursor_tick,
                        });
                    }
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
                // Double-click on a clip opens the piano roll (only when idle)
                if let Some(app) = cx.data::<AppData>() {
                    if matches!(app.arrangement.interaction, ClipInteraction::Idle) {
                        let cursor_x = cx.mouse().cursor_x;
                        let track_clips = Self::track_clips(app, self.track_index);
                        if let Some((clip_id, _)) = hit_test_clip_zone(
                            &track_clips,
                            app.arrangement.zoom_x,
                            app.arrangement.scroll_x,
                            cursor_x,
                            cx.bounds().x,
                        ) {
                            cx.emit(AppEvent::OpenPianoRoll(clip_id));
                        }
                    }
                }
                meta.consume();
            }
            WindowEvent::MouseDown(MouseButton::Left) => {
                let cursor_x = cx.mouse().cursor_x;
                let cursor_y = cx.mouse().cursor_y;
                let bounds = cx.bounds();
                let shift = cx.modifiers().contains(Modifiers::SHIFT);

                // Extract data from app before emitting events
                let action = if let Some(app) = cx.data::<AppData>() {
                    let track = app.tracks.get(self.track_index);
                    track.map(|t| {
                        let track_id = t.id;
                        let track_clips = Self::track_clips(app, self.track_index);
                        let hit = hit_test_clip_zone(
                            &track_clips,
                            app.arrangement.zoom_x,
                            app.arrangement.scroll_x,
                            cursor_x,
                            bounds.x,
                        );
                        let click_tick = app.arrangement.x_to_tick(cursor_x - bounds.x);
                        (track_id, hit, click_tick)
                    })
                } else {
                    None
                };

                if let Some((track_id, hit, click_tick)) = action {
                    cx.emit(AppEvent::SelectTrack(track_id));

                    match hit {
                        Some((clip_id, zone)) => {
                            cx.emit(AppEvent::UpdateClipInteraction(
                                ClipInteraction::PendingDrag {
                                    clip_id,
                                    track_id,
                                    mouse_start_x: cursor_x,
                                    mouse_start_y: cursor_y,
                                    click_tick,
                                    hit_zone: zone,
                                },
                            ));
                            cx.capture();
                        }
                        None => {
                            if !shift {
                                cx.emit(AppEvent::SelectClips(ClipSelection::default()));
                            }
                            cx.emit(AppEvent::UpdateClipInteraction(
                                ClipInteraction::RubberBand {
                                    origin_x: cursor_x,
                                    origin_y: cursor_y,
                                    current_x: cursor_x,
                                    current_y: cursor_y,
                                },
                            ));
                            cx.capture();
                        }
                    }
                }
                cx.needs_redraw();
                meta.consume();
            }
            WindowEvent::MouseMove(mx, my) => {
                // Compute events to emit from app state, then drop the borrow
                // before calling cx.emit() (which needs &mut cx).
                let events = if let Some(app) = cx.data::<AppData>() {
                    let bounds = cx.bounds();
                    compute_mouse_move_events(app, self.track_index, bounds, *mx, *my)
                } else {
                    Vec::new()
                };
                if !events.is_empty() {
                    for ev in events {
                        cx.emit(ev);
                    }
                    cx.needs_redraw();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                let shift = cx.modifiers().contains(Modifiers::SHIFT);

                // Extract events to emit from current interaction state
                let events = if let Some(app) = cx.data::<AppData>() {
                    compute_mouse_up_events(app, shift)
                } else {
                    Vec::new()
                };
                for ev in events {
                    cx.emit(ev);
                }
                cx.emit(AppEvent::UpdateClipInteraction(ClipInteraction::Idle));
                cx.release();
                cx.needs_redraw();
                meta.consume();
            }
            _ => {}
        });
    }
}

/// Compute events to emit from a MouseMove, without holding &mut cx.
/// Returns a list of AppEvents that should be emitted after the borrow is dropped.
fn compute_mouse_move_events(
    app: &AppData,
    track_index: usize,
    bounds: BoundingBox,
    mx: f32,
    my: f32,
) -> Vec<AppEvent> {
    let mut events = Vec::new();

    match app.arrangement.interaction.clone() {
        ClipInteraction::PendingDrag {
            clip_id,
            mouse_start_x,
            mouse_start_y,
            hit_zone,
            ..
        } => {
            let dx = mx - mouse_start_x;
            let dy = my - mouse_start_y;
            if dx.abs() > DRAG_THRESHOLD || dy.abs() > DRAG_THRESHOLD {
                match hit_zone {
                    ClipHitZone::Body => {
                        if let Some(clip) = app.clip(clip_id) {
                            let click_tick = app.arrangement.x_to_tick(mouse_start_x - bounds.x);
                            let grab_offset = click_tick - clip.start_tick;

                            if !app.arrangement.selected_clips.contains(&clip_id) {
                                events.push(AppEvent::SelectClips(ClipSelection::select_single(
                                    clip_id,
                                )));
                            }

                            events.push(AppEvent::UpdateClipInteraction(
                                ClipInteraction::MovingClips {
                                    anchor_clip_id: clip_id,
                                    anchor_original_start: clip.start_tick,
                                    anchor_original_track: clip.track_id,
                                    grab_offset_tick: grab_offset,
                                    delta_tick: 0,
                                    delta_track_index: 0,
                                },
                            ));
                        }
                    }
                    ClipHitZone::LeftEdge | ClipHitZone::RightEdge => {
                        if let Some(clip) = app.clip(clip_id) {
                            let edge = if hit_zone == ClipHitZone::LeftEdge {
                                clip_interaction::ClipResizeEdge::Left
                            } else {
                                clip_interaction::ClipResizeEdge::Right
                            };
                            events.push(AppEvent::UpdateClipInteraction(
                                ClipInteraction::ResizingClip {
                                    clip_id,
                                    edge,
                                    original_start: clip.start_tick,
                                    original_duration: clip.duration_ticks,
                                },
                            ));
                        }
                    }
                }
            }
        }
        ClipInteraction::MovingClips {
            anchor_clip_id,
            anchor_original_start,
            anchor_original_track,
            grab_offset_tick,
            ..
        } => {
            let raw_tick = app.arrangement.x_to_tick(mx - bounds.x);
            let beats = app.transport.time_signature.numerator;
            let snapped = app
                .arrangement
                .snap_grid
                .snap_floor(raw_tick - grab_offset_tick, beats);
            let delta_tick = snapped - anchor_original_start;

            let mouse_track_idx = track_index as i32;
            let original_idx = app
                .tracks
                .iter()
                .position(|t| t.id == anchor_original_track)
                .unwrap_or(0) as i32;
            let delta_track = mouse_track_idx - original_idx;

            events.push(AppEvent::UpdateClipInteraction(
                ClipInteraction::MovingClips {
                    anchor_clip_id,
                    anchor_original_start,
                    anchor_original_track,
                    grab_offset_tick,
                    delta_tick,
                    delta_track_index: delta_track,
                },
            ));
        }
        ClipInteraction::ResizingClip {
            clip_id,
            edge,
            original_start,
            original_duration,
        } => {
            let raw_tick = app.arrangement.x_to_tick(mx - bounds.x);
            let beats = app.transport.time_signature.numerator;

            let (new_start, new_duration) = match edge {
                clip_interaction::ClipResizeEdge::Right => {
                    let snapped_end = app.arrangement.snap_grid.snap(raw_tick, beats);
                    let dur =
                        (snapped_end - original_start).max(clip_interaction::MIN_CLIP_DURATION);
                    (original_start, dur)
                }
                clip_interaction::ClipResizeEdge::Left => {
                    let snapped_start = app.arrangement.snap_grid.snap_floor(raw_tick, beats);
                    let original_end = original_start + original_duration;
                    let new_s = snapped_start
                        .max(0)
                        .min(original_end - clip_interaction::MIN_CLIP_DURATION);
                    (new_s, original_end - new_s)
                }
            };

            events.push(AppEvent::UpdateClipInteraction(
                ClipInteraction::ResizingClip {
                    clip_id,
                    edge,
                    original_start: new_start,
                    original_duration: new_duration,
                },
            ));
        }
        ClipInteraction::RubberBand {
            origin_x, origin_y, ..
        } => {
            events.push(AppEvent::UpdateClipInteraction(
                ClipInteraction::RubberBand {
                    origin_x,
                    origin_y,
                    current_x: mx,
                    current_y: my,
                },
            ));

            // Live selection
            let arrangement = &app.arrangement;
            let lane_x = bounds.x;

            let tick1 = arrangement.x_to_tick((origin_x - lane_x).max(0.0));
            let tick2 = arrangement.x_to_tick((mx - lane_x).max(0.0));
            let tick_start = tick1.min(tick2);
            let tick_end = tick1.max(tick2);

            let track_height = arrangement.track_height;
            let y_min = origin_y.min(my);
            let y_max = origin_y.max(my);

            // Derive arrangement top Y from this lane's known position
            let arrangement_top_y = bounds.y - (track_index as f32 * track_height);
            let t_start = ((y_min - arrangement_top_y) / track_height)
                .floor()
                .max(0.0) as usize;
            let t_end = ((y_max - arrangement_top_y) / track_height)
                .floor()
                .max(0.0) as usize;

            let track_map: Vec<_> = app
                .tracks
                .iter()
                .enumerate()
                .map(|(i, t)| (t.id, i))
                .collect();

            let rect = SelectionRect {
                tick_start,
                tick_end,
                track_start: t_start,
                track_end: t_end,
            };

            let selected = clips_in_rect(&app.clips, &track_map, &rect);
            events.push(AppEvent::SelectClips(ClipSelection { clips: selected }));
        }
        ClipInteraction::Idle => {}
    }

    events
}

/// Compute events to emit from a MouseUp.
fn compute_mouse_up_events(app: &AppData, shift: bool) -> Vec<AppEvent> {
    let mut events = Vec::new();
    match &app.arrangement.interaction {
        ClipInteraction::PendingDrag { clip_id, .. } => {
            let clip_id = *clip_id;
            if shift {
                events.push(AppEvent::SelectClips(
                    app.arrangement.selected_clips.toggled(clip_id),
                ));
            } else {
                events.push(AppEvent::SelectClips(ClipSelection::select_single(clip_id)));
            }
        }
        ClipInteraction::MovingClips {
            delta_tick,
            delta_track_index,
            ..
        } => {
            if *delta_tick != 0 || *delta_track_index != 0 {
                events.push(AppEvent::MoveClips {
                    delta_tick: *delta_tick,
                    delta_track_index: *delta_track_index,
                });
            }
        }
        ClipInteraction::ResizingClip {
            clip_id,
            original_start,
            original_duration,
            ..
        } => {
            events.push(AppEvent::ResizeClip {
                clip_id: *clip_id,
                new_start: *original_start,
                new_duration: *original_duration,
            });
        }
        ClipInteraction::RubberBand { .. } | ClipInteraction::Idle => {}
    }
    events
}

/// Draw a rubber-band selection rectangle.
fn draw_rubber_band(canvas: &Canvas, x1: f32, y1: f32, x2: f32, y2: f32, scale: f32) {
    let left = x1.min(x2);
    let top = y1.min(y2);
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();

    if w < 1.0 || h < 1.0 {
        return;
    }

    let rect = vg::Rect::from_xywh(left, top, w, h);

    let mut fill = vg::Paint::default();
    fill.set_color(vg::Color::from_argb(30, 80, 160, 255));
    fill.set_style(vg::PaintStyle::Fill);
    fill.set_anti_alias(true);
    canvas.draw_rect(rect, &fill);

    let mut stroke = vg::Paint::default();
    stroke.set_color(vg::Color::from_argb(180, 80, 160, 255));
    stroke.set_style(vg::PaintStyle::Stroke);
    stroke.set_stroke_width(1.0 * scale);
    stroke.set_anti_alias(true);
    canvas.draw_rect(rect, &stroke);
}

/// Hit-test clips at a pixel position. Returns the first clip containing x.
#[cfg(test)]
fn hit_test_clip(
    clips: &[ClipState],
    zoom_x: f64,
    scroll_x: f64,
    x: f32,
    bounds_x: f32,
) -> Option<ClipId> {
    for clip in clips {
        let clip_x = bounds_x + ((clip.start_tick as f64 - scroll_x) * zoom_x) as f32;
        let clip_w = (clip.duration_ticks as f64 * zoom_x) as f32;
        if x >= clip_x && x <= clip_x + clip_w {
            return Some(clip.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::arrangement_state::ArrangementState;
    use crate::types::track::TrackId;
    use uuid::Uuid;

    fn test_clip_id(n: u128) -> ClipId {
        ClipId(Uuid::from_u128(n))
    }

    fn test_track_id() -> TrackId {
        TrackId(Uuid::from_u128(999))
    }

    fn make_clip(id: u128, start: i64, duration: i64) -> ClipState {
        ClipState {
            id: test_clip_id(id),
            track_id: test_track_id(),
            start_tick: start,
            duration_ticks: duration,
            name: format!("Clip {id}"),
            notes: Vec::new(),
            audio_file: None,
            audio_length_samples: None,
            audio_sample_rate: None,
        }
    }

    // -- Coordinate conversion tests (ArrangementState) --

    #[test]
    fn tick_to_x_at_origin() {
        let state = ArrangementState::default();
        let x = state.tick_to_x(0);
        assert!((x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn tick_to_x_positive() {
        let state = ArrangementState {
            zoom_x: 0.1,
            scroll_x: 0.0,
            ..Default::default()
        };
        let x = state.tick_to_x(960);
        assert!((x - 96.0).abs() < 0.01);
    }

    #[test]
    fn tick_to_x_with_scroll() {
        let state = ArrangementState {
            zoom_x: 0.1,
            scroll_x: 480.0,
            ..Default::default()
        };
        let x = state.tick_to_x(960);
        assert!((x - 48.0).abs() < 0.01);
    }

    #[test]
    fn x_to_tick_roundtrip() {
        let state = ArrangementState {
            zoom_x: 0.1,
            scroll_x: 200.0,
            ..Default::default()
        };
        let tick = 960_i64;
        let x = state.tick_to_x(tick);
        let recovered = state.x_to_tick(x);
        assert_eq!(recovered, tick);
    }

    #[test]
    fn x_to_tick_roundtrip_high_zoom() {
        let state = ArrangementState {
            zoom_x: 0.5,
            scroll_x: 1000.0,
            ..Default::default()
        };
        let tick = 5000_i64;
        let x = state.tick_to_x(tick);
        let recovered = state.x_to_tick(x);
        assert_eq!(recovered, tick);
    }

    // -- Clip hit testing --

    #[test]
    fn hit_test_clip_inside() {
        let clips = vec![make_clip(1, 0, 960)];
        let result = hit_test_clip(&clips, 0.1, 0.0, 50.0, 0.0);
        assert_eq!(result, Some(test_clip_id(1)));
    }

    #[test]
    fn hit_test_clip_outside_right() {
        let clips = vec![make_clip(1, 0, 960)];
        let result = hit_test_clip(&clips, 0.1, 0.0, 100.0, 0.0);
        assert_eq!(result, None);
    }

    #[test]
    fn hit_test_clip_outside_left() {
        let clips = vec![make_clip(1, 960, 960)];
        let result = hit_test_clip(&clips, 0.1, 0.0, 50.0, 0.0);
        assert_eq!(result, None);
    }

    #[test]
    fn hit_test_clip_at_boundary() {
        let clips = vec![make_clip(1, 0, 960)];
        let result = hit_test_clip(&clips, 0.1, 0.0, 96.0, 0.0);
        assert_eq!(result, Some(test_clip_id(1)));
    }

    #[test]
    fn hit_test_clip_with_scroll() {
        let clips = vec![make_clip(1, 960, 960)];
        let result = hit_test_clip(&clips, 0.1, 960.0, 50.0, 0.0);
        assert_eq!(result, Some(test_clip_id(1)));
    }

    #[test]
    fn hit_test_clip_with_bounds_offset() {
        let clips = vec![make_clip(1, 0, 960)];
        let result = hit_test_clip(&clips, 0.1, 0.0, 150.0, 100.0);
        assert_eq!(result, Some(test_clip_id(1)));
    }

    #[test]
    fn hit_test_clip_between_two_clips() {
        let clips = vec![make_clip(1, 0, 480), make_clip(2, 960, 480)];
        let result = hit_test_clip(&clips, 0.1, 0.0, 70.0, 0.0);
        assert_eq!(result, None);
    }

    #[test]
    fn hit_test_clip_second_of_two() {
        let clips = vec![make_clip(1, 0, 480), make_clip(2, 960, 480)];
        let result = hit_test_clip(&clips, 0.1, 0.0, 100.0, 0.0);
        assert_eq!(result, Some(test_clip_id(2)));
    }

    #[test]
    fn hit_test_empty_clips() {
        let clips: Vec<ClipState> = Vec::new();
        let result = hit_test_clip(&clips, 0.1, 0.0, 50.0, 0.0);
        assert_eq!(result, None);
    }
}
