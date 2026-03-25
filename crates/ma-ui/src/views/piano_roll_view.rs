//! Piano Roll View -- MIDI note editor with grid, keyboard strip, and mouse interaction FSM.
//!
//! Composed of:
//! - A quantize toolbar (grid resolution buttons)
//! - A KeyboardStrip (left sidebar, 60px)
//! - A PianoRollGrid (main editable area with notes, grid lines, and playhead)

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::state::piano_roll_state::{PianoRollInteraction, ResizeEdge};
use crate::types::midi::{is_black_key, Note, NoteId};
use crate::types::time::{QuantizeGrid, Tick, PPQN};
use crate::widgets::keyboard_strip::KeyboardStrip;

/// Keyboard strip width in logical pixels.
const KEYBOARD_WIDTH: f32 = 60.0;

/// Resize handle hit zone at the right edge of a note (in pixels).
const RESIZE_HIT_ZONE: f32 = 6.0;

/// Minimum note duration after resize (in ticks).
const MIN_NOTE_DURATION: Tick = 1;

// ---------------------------------------------------------------------------
// PianoRollView -- outer layout
// ---------------------------------------------------------------------------

/// Full piano roll view: toolbar + keyboard strip + grid.
pub struct PianoRollView;

impl PianoRollView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            // -- Quantize toolbar --
            HStack::new(cx, |cx| {
                Label::new(cx, "Quantize:").class("quantize-label");

                Self::quantize_button(cx, "Off", QuantizeGrid::Off);
                Self::quantize_button(cx, "1/4", QuantizeGrid::Quarter);
                Self::quantize_button(cx, "1/8", QuantizeGrid::Eighth);
                Self::quantize_button(cx, "1/16", QuantizeGrid::Sixteenth);
                Self::quantize_button(cx, "1/32", QuantizeGrid::ThirtySecond);
            })
            .class("quantize-toolbar");

            // -- Main content: keyboard + grid --
            HStack::new(cx, |cx| {
                KeyboardStrip::new(cx).width(Pixels(KEYBOARD_WIDTH));

                PianoRollGrid::new(cx);
            })
            .class("piano-roll-content");
        })
    }

    /// Helper to build a quantize grid button.
    fn quantize_button(cx: &mut Context, label: &str, grid: QuantizeGrid) {
        let label_str = label.to_string();
        Button::new(cx, move |cx| Label::new(cx, &label_str))
            .on_press(move |cx| cx.emit(AppEvent::SetQuantize(grid)))
            .class("quantize-btn")
            .toggle_class(
                "active",
                AppData::piano_roll.map(move |pr| pr.quantize == grid),
            );
    }
}

impl View for PianoRollView {
    fn element(&self) -> Option<&'static str> {
        Some("piano-roll-view")
    }
}

// ---------------------------------------------------------------------------
// PianoRollGrid -- the core drawing + interaction canvas
// ---------------------------------------------------------------------------

/// The editable MIDI note grid -- handles drawing and the full mouse interaction FSM.
struct PianoRollGrid;

impl PianoRollGrid {
    fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |_cx| {})
    }

    // -- Drawing helpers --------------------------------------------------

    /// Draw pitch row backgrounds with alternating shading for black/white keys.
    fn draw_pitch_rows(app: &AppData, bounds: BoundingBox, scale: f32, canvas: &Canvas) {
        let pr = &app.piano_roll;
        let note_height = pr.note_height;
        let visible_rows = pr.visible_rows(bounds.h) + 2;

        let mut black_row_paint = vg::Paint::default();
        black_row_paint.set_color(vg::Color::from_argb(255, 37, 37, 37));
        black_row_paint.set_style(vg::PaintStyle::Fill);
        black_row_paint.set_anti_alias(true);

        let mut line_paint = vg::Paint::default();
        line_paint.set_color(vg::Color::from_argb(255, 51, 51, 51));
        line_paint.set_style(vg::PaintStyle::Stroke);
        line_paint.set_stroke_width(0.5 * scale);
        line_paint.set_anti_alias(true);

        let mut c_line_paint = vg::Paint::default();
        c_line_paint.set_color(vg::Color::from_argb(255, 68, 68, 68));
        c_line_paint.set_style(vg::PaintStyle::Stroke);
        c_line_paint.set_stroke_width(1.0 * scale);
        c_line_paint.set_anti_alias(true);

        for i in 0..visible_rows {
            let pitch_i32 = pr.scroll_y as i32 - i as i32;
            if !(0..=127).contains(&pitch_i32) {
                continue;
            }
            let pitch = pitch_i32 as u8;
            let y = pr.pitch_to_y(pitch, bounds.y);
            let row_bottom = y + note_height;

            if row_bottom < bounds.y || y > bounds.y + bounds.h {
                continue;
            }

            // Black key rows get a slightly lighter background
            if is_black_key(pitch) {
                canvas.draw_rect(
                    vg::Rect::from_xywh(bounds.x, y, bounds.w, note_height),
                    &black_row_paint,
                );
            }

            // Horizontal separator line
            let paint = if pitch.is_multiple_of(12) {
                &c_line_paint
            } else {
                &line_paint
            };
            canvas.draw_line(
                (bounds.x, row_bottom),
                (bounds.x + bounds.w, row_bottom),
                paint,
            );
        }
    }

    /// Draw vertical grid lines (bars, beats, quantize subdivisions).
    fn draw_grid_lines(app: &AppData, bounds: BoundingBox, scale: f32, canvas: &Canvas) {
        let pr = &app.piano_roll;
        let time_sig = app.transport.time_signature;
        let ticks_per_beat = PPQN;
        let ticks_per_bar = ticks_per_beat * time_sig.numerator as i64;
        let quantize_ticks = pr.quantize.ticks();

        // Visible tick range
        let visible_ticks = if pr.zoom_x > 0.0 {
            (bounds.w as f64 / pr.zoom_x) as i64
        } else {
            return;
        };
        let start_tick = pr.scroll_x as i64;
        let end_tick = start_tick + visible_ticks;

        let pixels_per_bar = (ticks_per_bar as f64 * pr.zoom_x) as f32;
        let pixels_per_beat = (ticks_per_beat as f64 * pr.zoom_x) as f32;
        let pixels_per_quantize = (quantize_ticks as f64 * pr.zoom_x) as f32;

        // -- Quantize grid lines (very faint) --
        if pixels_per_quantize >= 4.0 * scale && quantize_ticks < ticks_per_beat {
            let mut q_paint = vg::Paint::default();
            q_paint.set_color(vg::Color::from_argb(255, 40, 40, 40));
            q_paint.set_style(vg::PaintStyle::Stroke);
            q_paint.set_stroke_width(0.5 * scale);
            q_paint.set_anti_alias(true);

            let first_q = if start_tick > 0 {
                (start_tick / quantize_ticks) * quantize_ticks
            } else {
                0
            };
            let mut q_tick = first_q;
            while q_tick <= end_tick + quantize_ticks {
                // Skip ticks that fall on beat or bar lines (drawn separately)
                if q_tick % ticks_per_beat != 0 {
                    let x = bounds.x + pr.tick_to_x(q_tick);
                    if x >= bounds.x && x <= bounds.x + bounds.w {
                        canvas.draw_line((x, bounds.y), (x, bounds.y + bounds.h), &q_paint);
                    }
                }
                q_tick += quantize_ticks;
            }
        }

        // -- Beat lines (subtle) --
        if pixels_per_beat >= 6.0 * scale {
            let mut beat_paint = vg::Paint::default();
            beat_paint.set_color(vg::Color::from_argb(255, 51, 51, 51));
            beat_paint.set_style(vg::PaintStyle::Stroke);
            beat_paint.set_stroke_width(0.5 * scale);
            beat_paint.set_anti_alias(true);

            let first_beat = if start_tick > 0 {
                (start_tick / ticks_per_beat) * ticks_per_beat
            } else {
                0
            };
            let mut beat_tick = first_beat;
            while beat_tick <= end_tick + ticks_per_beat {
                // Skip bar lines (drawn brighter)
                if beat_tick % ticks_per_bar != 0 {
                    let x = bounds.x + pr.tick_to_x(beat_tick);
                    if x >= bounds.x && x <= bounds.x + bounds.w {
                        canvas.draw_line((x, bounds.y), (x, bounds.y + bounds.h), &beat_paint);
                    }
                }
                beat_tick += ticks_per_beat;
            }
        }

        // -- Bar lines (brighter) --
        let mut bar_paint = vg::Paint::default();
        bar_paint.set_color(vg::Color::from_argb(255, 85, 85, 85));
        bar_paint.set_style(vg::PaintStyle::Stroke);
        bar_paint.set_stroke_width(1.0 * scale);
        bar_paint.set_anti_alias(true);

        let first_bar = if start_tick > 0 {
            (start_tick / ticks_per_bar) * ticks_per_bar
        } else {
            0
        };
        let mut bar_tick = first_bar;
        while bar_tick <= end_tick + ticks_per_bar {
            let x = bounds.x + pr.tick_to_x(bar_tick);
            if x >= bounds.x && x <= bounds.x + bounds.w {
                canvas.draw_line((x, bounds.y), (x, bounds.y + bounds.h), &bar_paint);
            }
            bar_tick += ticks_per_bar;
        }

        // -- Bar number labels --
        if pixels_per_bar > 0.0 {
            let min_label_spacing = 50.0 * scale;
            let bar_label_step = ((min_label_spacing / pixels_per_bar).ceil() as i64).max(1);

            let mut text_paint = vg::Paint::default();
            text_paint.set_color(vg::Color::from_argb(180, 120, 120, 120));
            text_paint.set_anti_alias(true);

            let font = vg::Font::default();

            let mut bt = first_bar;
            while bt <= end_tick + ticks_per_bar {
                let bar_num = bt / ticks_per_bar + 1;
                if (bar_num - 1) % bar_label_step == 0 {
                    let x = bounds.x + pr.tick_to_x(bt);
                    if x >= bounds.x && x <= bounds.x + bounds.w {
                        let label = format!("{}", bar_num);
                        canvas.draw_str(
                            &label,
                            (x + 2.0 * scale, bounds.y + 12.0 * scale),
                            &font,
                            &text_paint,
                        );
                    }
                }
                bt += ticks_per_bar;
            }
        }
    }

    /// Draw MIDI notes for the active clip.
    fn draw_notes(app: &AppData, bounds: BoundingBox, scale: f32, canvas: &Canvas) {
        let pr = &app.piano_roll;
        let clip_id = match pr.active_clip_id {
            Some(id) => id,
            None => return,
        };

        let notes = match app.clips.iter().find(|c| c.id == clip_id) {
            Some(clip) => &clip.notes,
            None => return,
        };

        // Determine track color for note rendering
        let track_color = app
            .clips
            .iter()
            .find(|c| c.id == clip_id)
            .and_then(|clip| app.tracks.iter().find(|t| t.id == clip.track_id))
            .map(|t| t.color)
            .unwrap_or([100, 160, 255]);

        let visible_rows = pr.visible_rows(bounds.h) + 2;
        let min_visible_pitch = (pr.scroll_y as i32 - visible_rows as i32).max(0) as u8;
        let max_visible_pitch = pr.scroll_y;

        let visible_start_tick = pr.x_to_tick(0.0);
        let visible_end_tick = pr.x_to_tick(bounds.w);

        let mut note_paint = vg::Paint::default();
        note_paint.set_style(vg::PaintStyle::Fill);
        note_paint.set_anti_alias(true);

        let mut border_paint = vg::Paint::default();
        border_paint.set_color(vg::Color::from_argb(200, 0, 0, 0));
        border_paint.set_style(vg::PaintStyle::Stroke);
        border_paint.set_stroke_width(0.5 * scale);
        border_paint.set_anti_alias(true);

        let mut selected_border_paint = vg::Paint::default();
        selected_border_paint.set_color(vg::Color::from_argb(255, 255, 255, 255));
        selected_border_paint.set_style(vg::PaintStyle::Stroke);
        selected_border_paint.set_stroke_width(2.0 * scale);
        selected_border_paint.set_anti_alias(true);

        for note in notes.iter() {
            // Visibility culling
            if note.pitch < min_visible_pitch || note.pitch > max_visible_pitch {
                continue;
            }
            if note.end_tick() < visible_start_tick || note.start_tick > visible_end_tick {
                continue;
            }

            let x = bounds.x + pr.tick_to_x(note.start_tick);
            let y = pr.pitch_to_y(note.pitch, bounds.y);
            let w = ((note.duration_ticks as f64) * pr.zoom_x) as f32;
            let h = pr.note_height;

            // Velocity-based alpha: map 0..127 to 120..255
            let alpha = 120 + ((note.velocity as u32 * 135) / 127) as u8;

            note_paint.set_color(vg::Color::from_argb(
                alpha,
                track_color[0],
                track_color[1],
                track_color[2],
            ));

            let corner_radius = (2.0 * scale).min(h * 0.3);
            let note_rect = vg::Rect::from_xywh(x, y, w.max(1.0), h);
            let rrect = vg::RRect::new_rect_xy(note_rect, corner_radius, corner_radius);

            canvas.draw_rrect(rrect, &note_paint);
            canvas.draw_rrect(rrect, &border_paint);

            // Selected notes: white border
            let is_selected = pr.selected_notes.contains(&note.id);
            if is_selected {
                canvas.draw_rrect(rrect, &selected_border_paint);
            }
        }

        // -- Ghost note for DrawingNote interaction --
        if let PianoRollInteraction::DrawingNote {
            start_tick,
            pitch,
            velocity,
            current_end_tick,
        } = &pr.interaction
        {
            let draw_start = (*start_tick).min(*current_end_tick);
            let draw_end = (*start_tick).max(*current_end_tick);
            let duration = draw_end - draw_start;

            let x = bounds.x + pr.tick_to_x(draw_start);
            let y = pr.pitch_to_y(*pitch, bounds.y);
            let w = ((duration as f64) * pr.zoom_x) as f32;
            let h = pr.note_height;

            let alpha = 80 + ((*velocity as u32 * 100) / 127) as u8;
            let mut ghost_paint = vg::Paint::default();
            ghost_paint.set_color(vg::Color::from_argb(
                alpha,
                track_color[0],
                track_color[1],
                track_color[2],
            ));
            ghost_paint.set_style(vg::PaintStyle::Fill);
            ghost_paint.set_anti_alias(true);

            let corner_radius = (2.0 * scale).min(h * 0.3);
            let ghost_rect = vg::Rect::from_xywh(x, y, w.max(1.0), h);
            let rrect = vg::RRect::new_rect_xy(ghost_rect, corner_radius, corner_radius);
            canvas.draw_rrect(rrect, &ghost_paint);

            // Dashed border for ghost note
            let mut ghost_border = vg::Paint::default();
            ghost_border.set_color(vg::Color::from_argb(200, 255, 255, 255));
            ghost_border.set_style(vg::PaintStyle::Stroke);
            ghost_border.set_stroke_width(1.0 * scale);
            ghost_border.set_anti_alias(true);
            canvas.draw_rrect(rrect, &ghost_border);
        }
    }

    /// Draw the transport playhead as a vertical red line.
    fn draw_playhead(app: &AppData, bounds: BoundingBox, scale: f32, canvas: &Canvas) {
        let pr = &app.piano_roll;
        let position = app.transport.position;

        let x = bounds.x + pr.tick_to_x(position);
        if x < bounds.x || x > bounds.x + bounds.w {
            return;
        }

        let mut playhead_paint = vg::Paint::default();
        playhead_paint.set_color(vg::Color::from_argb(255, 255, 68, 68));
        playhead_paint.set_style(vg::PaintStyle::Stroke);
        playhead_paint.set_stroke_width(2.0 * scale);
        playhead_paint.set_anti_alias(true);

        canvas.draw_line((x, bounds.y), (x, bounds.y + bounds.h), &playhead_paint);
    }

    // -- Hit testing helpers ----------------------------------------------

    /// Find the note under the given screen coordinates. Returns in reverse
    /// order (topmost note first) so recently-drawn notes take priority.
    fn hit_test_note(
        app: &AppData,
        bounds: BoundingBox,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<(Note, bool)> {
        let pr = &app.piano_roll;
        let clip_id = pr.active_clip_id?;
        let clip = app.clips.iter().find(|c| c.id == clip_id)?;

        let rel_x = mouse_x - bounds.x;
        let rel_y = mouse_y - bounds.y;

        hit_test_note_in_slice(
            &clip.notes,
            &NoteHitTestParams {
                zoom_x: pr.zoom_x,
                scroll_x: pr.scroll_x,
                scroll_y: pr.scroll_y,
                note_height: pr.note_height,
                resize_hit_zone: RESIZE_HIT_ZONE,
            },
            rel_x,
            rel_y,
        )
    }

    /// Convert screen coordinates to (tick, pitch).
    fn screen_to_tick_pitch(
        app: &AppData,
        bounds: BoundingBox,
        mouse_x: f32,
        mouse_y: f32,
    ) -> (Tick, u8) {
        let pr = &app.piano_roll;
        let rel_x = mouse_x - bounds.x;
        let tick = pr.x_to_tick(rel_x);
        let pitch = pr.y_to_pitch(mouse_y, bounds.y);
        (tick, pitch)
    }
}

impl View for PianoRollGrid {
    fn element(&self) -> Option<&'static str> {
        Some("piano-roll-grid")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        let Some(app) = cx.data::<AppData>() else {
            return;
        };

        // 1. Background
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(255, 30, 30, 30));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg_paint,
        );

        // 2. Pitch rows (alternating shading)
        Self::draw_pitch_rows(app, bounds, scale, canvas);

        // 3. Vertical grid lines
        Self::draw_grid_lines(app, bounds, scale, canvas);

        // 4. Notes
        Self::draw_notes(app, bounds, scale, canvas);

        // 5. Playhead
        Self::draw_playhead(app, bounds, scale, canvas);
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let mouse_x = cx.mouse().cursor_x;
                let mouse_y = cx.mouse().cursor_y;

                let Some(app) = cx.data::<AppData>() else {
                    return;
                };

                // Only respond if we have an active clip
                if app.piano_roll.active_clip_id.is_none() {
                    return;
                }

                let quantize = app.piano_roll.quantize;
                let default_velocity = app.piano_roll.default_velocity;

                match Self::hit_test_note(app, bounds, mouse_x, mouse_y) {
                    Some((note, near_right_edge)) => {
                        if near_right_edge {
                            // Start resizing from the right edge
                            cx.emit(AppEvent::UpdateInteraction(
                                PianoRollInteraction::ResizingNote {
                                    note_id: note.id,
                                    original_duration: note.duration_ticks,
                                    edge: ResizeEdge::Right,
                                },
                            ));
                        } else {
                            // Start dragging the note
                            let (click_tick, _) =
                                Self::screen_to_tick_pitch(app, bounds, mouse_x, mouse_y);
                            cx.emit(AppEvent::UpdateInteraction(
                                PianoRollInteraction::DraggingNote {
                                    note_id: note.id,
                                    original_start: note.start_tick,
                                    original_pitch: note.pitch,
                                    drag_offset_tick: click_tick - note.start_tick,
                                    drag_offset_pitch: 0,
                                },
                            ));
                            cx.emit(AppEvent::PreviewNoteOn {
                                note: note.pitch,
                                velocity: note.velocity,
                            });
                        }
                    }
                    None => {
                        // No note hit: start drawing a new note
                        let (raw_tick, pitch) =
                            Self::screen_to_tick_pitch(app, bounds, mouse_x, mouse_y);
                        let snapped_tick = quantize.snap_floor(raw_tick);

                        cx.emit(AppEvent::UpdateInteraction(
                            PianoRollInteraction::DrawingNote {
                                start_tick: snapped_tick,
                                pitch,
                                velocity: default_velocity,
                                current_end_tick: snapped_tick + quantize.ticks(),
                            },
                        ));
                        cx.emit(AppEvent::PreviewNoteOn {
                            note: pitch,
                            velocity: default_velocity,
                        });
                    }
                }

                cx.capture();
                cx.needs_redraw(); // REDRAW: on-change — note draw start
                meta.consume();
            }

            WindowEvent::MouseDown(MouseButton::Right) => {
                let bounds = cx.bounds();
                let mouse_x = cx.mouse().cursor_x;
                let mouse_y = cx.mouse().cursor_y;

                let Some(app) = cx.data::<AppData>() else {
                    return;
                };

                // Right-click on note: delete it
                if let Some((note, _)) = Self::hit_test_note(app, bounds, mouse_x, mouse_y) {
                    cx.emit(AppEvent::RemoveNote(note.id));
                }

                cx.needs_redraw(); // REDRAW: on-change — note delete
                meta.consume();
            }

            WindowEvent::MouseMove(mouse_x, mouse_y) => {
                let bounds = cx.bounds();

                let Some(app) = cx.data::<AppData>() else {
                    return;
                };

                let quantize = app.piano_roll.quantize;

                match &app.piano_roll.interaction {
                    PianoRollInteraction::DrawingNote {
                        start_tick,
                        pitch,
                        velocity,
                        ..
                    } => {
                        let (raw_tick, _) =
                            Self::screen_to_tick_pitch(app, bounds, *mouse_x, *mouse_y);
                        let snapped_end = quantize.snap(raw_tick);

                        cx.emit(AppEvent::UpdateInteraction(
                            PianoRollInteraction::DrawingNote {
                                start_tick: *start_tick,
                                pitch: *pitch,
                                velocity: *velocity,
                                current_end_tick: snapped_end,
                            },
                        ));
                        cx.needs_redraw(); // REDRAW: on-change — DrawingNote drag
                    }

                    PianoRollInteraction::DraggingNote {
                        note_id,
                        original_start: _,
                        original_pitch: _,
                        drag_offset_tick,
                        ..
                    } => {
                        let (raw_tick, new_pitch) =
                            Self::screen_to_tick_pitch(app, bounds, *mouse_x, *mouse_y);
                        let new_start = quantize.snap_floor(raw_tick - drag_offset_tick);
                        let clamped_start = new_start.max(0);

                        cx.emit(AppEvent::MoveNote {
                            note_id: *note_id,
                            new_start: clamped_start,
                            new_pitch,
                        });
                        cx.needs_redraw(); // REDRAW: on-change — DraggingNote move
                    }

                    PianoRollInteraction::ResizingNote {
                        note_id,
                        edge: ResizeEdge::Right,
                        ..
                    } => {
                        // Find the note's current start to compute new duration
                        let clip_id = app.piano_roll.active_clip_id;
                        let note_start = clip_id
                            .and_then(|cid| app.clips.iter().find(|c| c.id == cid))
                            .and_then(|clip| clip.notes.iter().find(|n| n.id == *note_id))
                            .map(|n| n.start_tick);

                        if let Some(start) = note_start {
                            let (raw_tick, _) =
                                Self::screen_to_tick_pitch(app, bounds, *mouse_x, *mouse_y);
                            let snapped_end = quantize.snap(raw_tick);
                            let new_duration = (snapped_end - start).max(MIN_NOTE_DURATION);

                            cx.emit(AppEvent::ResizeNote {
                                note_id: *note_id,
                                new_duration,
                            });
                            cx.needs_redraw(); // REDRAW: on-change — ResizingNote right edge
                        }
                    }

                    PianoRollInteraction::ResizingNote {
                        note_id,
                        edge: ResizeEdge::Left,
                        ..
                    } => {
                        // Left-edge resize: move start and adjust duration to keep end fixed
                        let clip_id = app.piano_roll.active_clip_id;
                        let the_note_id = *note_id;
                        let note_data = clip_id
                            .and_then(|cid| app.clips.iter().find(|c| c.id == cid))
                            .and_then(|clip| clip.notes.iter().find(|n| n.id == the_note_id))
                            .copied();
                        let (raw_tick, _) =
                            Self::screen_to_tick_pitch(app, bounds, *mouse_x, *mouse_y);

                        if let Some(note) = note_data {
                            let end_tick = note.end_tick();
                            let new_start = quantize.snap_floor(raw_tick).max(0);
                            let new_duration = (end_tick - new_start).max(MIN_NOTE_DURATION);

                            cx.emit(AppEvent::MoveNote {
                                note_id: the_note_id,
                                new_start,
                                new_pitch: note.pitch,
                            });
                            cx.emit(AppEvent::ResizeNote {
                                note_id: the_note_id,
                                new_duration,
                            });
                            cx.needs_redraw(); // REDRAW: on-change — ResizingNote left edge
                        }
                    }

                    PianoRollInteraction::Selecting { .. } | PianoRollInteraction::Idle => {}
                }
            }

            WindowEvent::MouseUp(MouseButton::Left) => {
                let Some(app) = cx.data::<AppData>() else {
                    return;
                };

                // Extract everything we need before dropping the borrow
                let interaction = app.piano_roll.interaction.clone();
                let grid_ticks = app.piano_roll.quantize.ticks();

                match &interaction {
                    PianoRollInteraction::DrawingNote {
                        start_tick,
                        pitch,
                        velocity,
                        current_end_tick,
                    } => {
                        let draw_start = (*start_tick).min(*current_end_tick);
                        let draw_end = (*start_tick).max(*current_end_tick);
                        let duration = (draw_end - draw_start).max(grid_ticks);

                        cx.emit(AppEvent::AddNote(Note {
                            id: NoteId(0),
                            pitch: *pitch,
                            start_tick: draw_start,
                            duration_ticks: duration,
                            velocity: *velocity,
                            channel: 0,
                        }));
                        cx.emit(AppEvent::PreviewNoteOff { note: *pitch });
                    }

                    PianoRollInteraction::DraggingNote { original_pitch, .. } => {
                        cx.emit(AppEvent::PreviewNoteOff {
                            note: *original_pitch,
                        });
                    }

                    PianoRollInteraction::ResizingNote { .. } => {}

                    PianoRollInteraction::Selecting { .. } | PianoRollInteraction::Idle => {}
                }

                cx.emit(AppEvent::UpdateInteraction(PianoRollInteraction::Idle));
                cx.release();
                cx.needs_redraw(); // REDRAW: on-change — end interaction
                meta.consume();
            }

            WindowEvent::MouseScroll(_dx, dy) => {
                let modifiers = cx.modifiers();

                if modifiers.contains(Modifiers::CTRL) {
                    let factor = if *dy > 0.0 { 1.1 } else { 0.9 };
                    cx.emit(AppEvent::ZoomPianoRoll(factor));
                } else if modifiers.contains(Modifiers::SHIFT) {
                    let delta = if *dy > 0.0 { 3i8 } else { -3 };
                    cx.emit(AppEvent::ScrollPianoRollY(delta));
                } else {
                    let scroll_amount = -*dy as f64 * 20.0;
                    cx.emit(AppEvent::ScrollPianoRollX(scroll_amount));
                }

                cx.needs_redraw(); // REDRAW: on-change — zoom/scroll
                meta.consume();
            }

            _ => {}
        });
    }
}

/// Parameters for note hit testing, extracted for testability.
struct NoteHitTestParams {
    zoom_x: f64,
    scroll_x: f64,
    scroll_y: u8,
    note_height: f32,
    resize_hit_zone: f32,
}

/// Hit-test notes in a slice. Returns (note, near_right_edge) for the topmost hit.
///
/// Extracted from `PianoRollGrid::hit_test_note` for testability.
/// `rel_x` and `rel_y` are relative to the grid origin (bounds top-left).
fn hit_test_note_in_slice(
    notes: &[Note],
    params: &NoteHitTestParams,
    rel_x: f32,
    rel_y: f32,
) -> Option<(Note, bool)> {
    for note in notes.iter().rev() {
        let nx = ((note.start_tick as f64 - params.scroll_x) * params.zoom_x) as f32;
        let ny = (params.scroll_y as f32 - note.pitch as f32) * params.note_height;
        let nw = ((note.duration_ticks as f64) * params.zoom_x) as f32;
        let nh = params.note_height;

        if rel_x >= nx && rel_x <= nx + nw && rel_y >= ny && rel_y <= ny + nh {
            let near_right_edge = rel_x >= nx + nw - params.resize_hit_zone;
            return Some((*note, near_right_edge));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::piano_roll_state::PianoRollState;
    use crate::types::midi::{Note, NoteId};

    fn make_note(id: u64, pitch: u8, start: i64, duration: i64) -> Note {
        Note {
            id: NoteId(id),
            pitch,
            start_tick: start,
            duration_ticks: duration,
            velocity: 100,
            channel: 0,
        }
    }

    // -- Coordinate conversion tests (PianoRollState) --

    #[test]
    fn tick_to_x_at_origin() {
        let pr = PianoRollState::default(); // zoom_x=0.15, scroll_x=0.0
        let x = pr.tick_to_x(0);
        assert!((x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn tick_to_x_positive() {
        let pr = PianoRollState {
            zoom_x: 0.2,
            scroll_x: 0.0,
            ..Default::default()
        };
        // 960 ticks * 0.2 px/tick = 192.0 px
        let x = pr.tick_to_x(960);
        assert!((x - 192.0).abs() < 0.01);
    }

    #[test]
    fn x_to_tick_roundtrip() {
        let pr = PianoRollState {
            zoom_x: 0.2,
            scroll_x: 500.0,
            ..Default::default()
        };
        let tick = 2000_i64;
        let x = pr.tick_to_x(tick);
        let recovered = pr.x_to_tick(x);
        assert_eq!(recovered, tick);
    }

    #[test]
    fn pitch_to_y_and_back() {
        let pr = PianoRollState {
            scroll_y: 84,
            note_height: 14.0,
            ..Default::default()
        };
        let rect_top = 0.0;
        let pitch = 60_u8; // C4

        let y = pr.pitch_to_y(pitch, rect_top);
        let recovered = pr.y_to_pitch(y + pr.note_height / 2.0, rect_top);
        assert_eq!(recovered, pitch);
    }

    #[test]
    fn pitch_to_y_higher_pitch_smaller_y() {
        let pr = PianoRollState::default();
        let y_c4 = pr.pitch_to_y(60, 0.0);
        let y_c5 = pr.pitch_to_y(72, 0.0);
        // Higher pitch → smaller y (higher on screen)
        assert!(y_c5 < y_c4);
    }

    #[test]
    fn y_to_pitch_clamps_to_valid_range() {
        let pr = PianoRollState::default();
        // Very large y → low pitch, clamped to 0
        let pitch = pr.y_to_pitch(10000.0, 0.0);
        assert_eq!(pitch, 0);
        // Negative y → high pitch, clamped to 127
        let pitch = pr.y_to_pitch(-10000.0, 0.0);
        assert_eq!(pitch, 127);
    }

    // -- Note hit testing --

    #[test]
    fn hit_test_note_body() {
        let notes = vec![make_note(1, 60, 0, 960)];
        let pr = PianoRollState {
            zoom_x: 0.2,
            scroll_x: 0.0,
            scroll_y: 84,
            note_height: 14.0,
            ..Default::default()
        };
        // Note at pitch 60: y = (84-60)*14 = 336.0, note spans x=[0, 192]
        let rel_x = 50.0;
        let rel_y = 340.0; // Inside the note row
        let result = hit_test_note_in_slice(
            &notes,
            &NoteHitTestParams {
                zoom_x: pr.zoom_x,
                scroll_x: pr.scroll_x,
                scroll_y: pr.scroll_y,
                note_height: pr.note_height,
                resize_hit_zone: RESIZE_HIT_ZONE,
            },
            rel_x,
            rel_y,
        );
        assert!(result.is_some());
        let (note, near_edge) = result.unwrap();
        assert_eq!(note.id, NoteId(1));
        assert!(!near_edge);
    }

    #[test]
    fn hit_test_note_right_edge_resize() {
        let notes = vec![make_note(1, 60, 0, 960)];
        let pr = PianoRollState {
            zoom_x: 0.2,
            scroll_x: 0.0,
            scroll_y: 84,
            note_height: 14.0,
            ..Default::default()
        };
        // Note right edge at x=192. Click at 190 (within RESIZE_HIT_ZONE=6)
        let rel_x = 190.0;
        let rel_y = 340.0;
        let result = hit_test_note_in_slice(
            &notes,
            &NoteHitTestParams {
                zoom_x: pr.zoom_x,
                scroll_x: pr.scroll_x,
                scroll_y: pr.scroll_y,
                note_height: pr.note_height,
                resize_hit_zone: RESIZE_HIT_ZONE,
            },
            rel_x,
            rel_y,
        );
        assert!(result.is_some());
        let (_, near_edge) = result.unwrap();
        assert!(near_edge);
    }

    #[test]
    fn hit_test_note_miss() {
        let notes = vec![make_note(1, 60, 0, 960)];
        let pr = PianoRollState {
            zoom_x: 0.2,
            scroll_x: 0.0,
            scroll_y: 84,
            note_height: 14.0,
            ..Default::default()
        };
        // Click far from any note
        let result = hit_test_note_in_slice(
            &notes,
            &NoteHitTestParams {
                zoom_x: pr.zoom_x,
                scroll_x: pr.scroll_x,
                scroll_y: pr.scroll_y,
                note_height: pr.note_height,
                resize_hit_zone: RESIZE_HIT_ZONE,
            },
            500.0,
            340.0,
        );
        assert!(result.is_none());
    }

    #[test]
    fn hit_test_note_topmost_wins() {
        // Two overlapping notes at the same position
        let notes = vec![make_note(1, 60, 0, 960), make_note(2, 60, 0, 960)];
        let pr = PianoRollState {
            zoom_x: 0.2,
            scroll_x: 0.0,
            scroll_y: 84,
            note_height: 14.0,
            ..Default::default()
        };
        let rel_x = 50.0;
        let rel_y = 340.0;
        let result = hit_test_note_in_slice(
            &notes,
            &NoteHitTestParams {
                zoom_x: pr.zoom_x,
                scroll_x: pr.scroll_x,
                scroll_y: pr.scroll_y,
                note_height: pr.note_height,
                resize_hit_zone: RESIZE_HIT_ZONE,
            },
            rel_x,
            rel_y,
        );
        assert!(result.is_some());
        // Last note in list (id=2) is "topmost" due to reverse iteration
        assert_eq!(result.unwrap().0.id, NoteId(2));
    }

    #[test]
    fn hit_test_empty_notes() {
        let notes: Vec<Note> = Vec::new();
        let result = hit_test_note_in_slice(
            &notes,
            &NoteHitTestParams {
                zoom_x: 0.2,
                scroll_x: 0.0,
                scroll_y: 84,
                note_height: 14.0,
                resize_hit_zone: RESIZE_HIT_ZONE,
            },
            50.0,
            340.0,
        );
        assert!(result.is_none());
    }

    #[test]
    fn hit_test_note_with_scroll() {
        let notes = vec![make_note(1, 60, 960, 960)];
        let pr = PianoRollState {
            zoom_x: 0.2,
            scroll_x: 960.0, // Scrolled to start of note
            scroll_y: 84,
            note_height: 14.0,
            ..Default::default()
        };
        // Note starts at tick 960, with scroll_x=960 → x starts at 0
        let rel_x = 50.0;
        let rel_y = (84.0 - 60.0) * 14.0 + 5.0; // Inside note row
        let result = hit_test_note_in_slice(
            &notes,
            &NoteHitTestParams {
                zoom_x: pr.zoom_x,
                scroll_x: pr.scroll_x,
                scroll_y: pr.scroll_y,
                note_height: pr.note_height,
                resize_hit_zone: RESIZE_HIT_ZONE,
            },
            rel_x,
            rel_y,
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, NoteId(1));
    }
}
