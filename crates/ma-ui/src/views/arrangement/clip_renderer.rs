//! Clip rendering — draw clip body, header, border, name, and MIDI note previews.

use vizia::prelude::*;
use vizia::vg;

use crate::types::track::ClipState;

/// Parameters for drawing clips within a track lane.
pub struct ClipDrawParams {
    pub bounds: BoundingBox,
    pub scale: f32,
    pub zoom_x: f64,
    pub scroll_x: f64,
    pub track_color: [u8; 3],
}

/// Draw a single clip (body, header bar, border, name text, and MIDI note rectangles).
pub fn draw_clip(canvas: &Canvas, clip: &ClipState, params: &ClipDrawParams, is_selected: bool) {
    let bounds = params.bounds;
    let scale = params.scale;
    let zoom_x = params.zoom_x;
    let scroll_x = params.scroll_x;
    let [tr, tg, tb] = params.track_color;

    let clip_x = bounds.x + ((clip.start_tick as f64 - scroll_x) * zoom_x) as f32;
    let clip_w = (clip.duration_ticks as f64 * zoom_x) as f32;
    let clip_end_x = clip_x + clip_w;

    // Skip clips entirely outside visible area
    if clip_end_x < bounds.x || clip_x > bounds.x + bounds.w {
        return;
    }

    // Clamp clip rect to visible area for drawing
    let draw_x = clip_x.max(bounds.x);
    let draw_end_x = clip_end_x.min(bounds.x + bounds.w);
    let draw_w = draw_end_x - draw_x;

    if draw_w <= 0.0 {
        return;
    }

    let clip_padding = 2.0 * scale;
    let clip_corner_radius = 3.0 * scale;
    let clip_y = bounds.y + clip_padding;
    let clip_h = bounds.h - clip_padding * 2.0;

    // -- Clip fill (semi-transparent track color) --
    let mut clip_fill = vg::Paint::default();
    clip_fill.set_color(vg::Color::from_argb(100, tr, tg, tb));
    clip_fill.set_style(vg::PaintStyle::Fill);
    clip_fill.set_anti_alias(true);

    let clip_rect = vg::Rect::from_xywh(draw_x, clip_y, draw_w, clip_h);
    let rrect = vg::RRect::new_rect_xy(clip_rect, clip_corner_radius, clip_corner_radius);
    canvas.draw_rrect(rrect, &clip_fill);

    // -- Clip border --
    let mut clip_border = vg::Paint::default();
    if is_selected {
        clip_border.set_color(vg::Color::from_argb(220, 255, 255, 255));
        clip_border.set_stroke_width(1.5 * scale);
    } else {
        clip_border.set_color(vg::Color::from_argb(140, tr, tg, tb));
        clip_border.set_stroke_width(1.0 * scale);
    }
    clip_border.set_style(vg::PaintStyle::Stroke);
    clip_border.set_anti_alias(true);
    canvas.draw_rrect(rrect, &clip_border);

    // -- Clip header bar (top strip with brighter color) --
    let header_h = 14.0 * scale;
    if clip_h > header_h + 2.0 {
        let mut header_paint = vg::Paint::default();
        header_paint.set_color(vg::Color::from_argb(160, tr, tg, tb));
        header_paint.set_style(vg::PaintStyle::Fill);
        header_paint.set_anti_alias(true);

        let header_rect = vg::Rect::from_xywh(draw_x, clip_y, draw_w, header_h);
        canvas.save();
        canvas.clip_rect(header_rect, None, Some(true));
        canvas.draw_rrect(rrect, &header_paint);
        canvas.restore();
    }

    // -- Clip name text (inside header area) --
    if draw_w > 20.0 * scale {
        let mut name_paint = vg::Paint::default();
        name_paint.set_color(vg::Color::from_argb(255, 240, 240, 240));
        name_paint.set_anti_alias(true);

        let clip_font = vg::Font::default();
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

    // -- Mini MIDI note rectangles (for MIDI clips with notes) --
    let geom = ClipGeometry {
        draw_x,
        draw_end_x,
        clip_y,
        clip_h,
        header_h,
    };
    draw_midi_notes(canvas, clip, params, &geom);
}

/// Computed clip geometry passed between drawing stages.
struct ClipGeometry {
    draw_x: f32,
    draw_end_x: f32,
    clip_y: f32,
    clip_h: f32,
    header_h: f32,
}

/// Draw miniature MIDI note rectangles inside a clip's note area.
fn draw_midi_notes(
    canvas: &Canvas,
    clip: &ClipState,
    params: &ClipDrawParams,
    geom: &ClipGeometry,
) {
    if clip.notes.is_empty() || geom.clip_h <= geom.header_h + 8.0 {
        return;
    }

    let [tr, tg, tb] = params.track_color;
    let note_area_y = geom.clip_y + geom.header_h + 1.0;
    let note_area_h = geom.clip_h - geom.header_h - 2.0;

    // Find pitch range in this clip
    let min_pitch = clip.notes.iter().map(|n| n.pitch).min().unwrap_or(0);
    let max_pitch = clip.notes.iter().map(|n| n.pitch).max().unwrap_or(127);
    let pitch_range = (max_pitch - min_pitch).max(1) as f32 + 2.0;

    let mut note_paint = vg::Paint::default();
    note_paint.set_color(vg::Color::from_argb(200, tr, tg, tb));
    note_paint.set_style(vg::PaintStyle::Fill);
    note_paint.set_anti_alias(true);

    canvas.save();
    canvas.clip_rect(
        vg::Rect::from_xywh(
            geom.draw_x,
            note_area_y,
            geom.draw_end_x - geom.draw_x,
            note_area_h,
        ),
        None,
        Some(true),
    );

    for note in &clip.notes {
        let note_x =
            params.bounds.x + ((note.start_tick as f64 - params.scroll_x) * params.zoom_x) as f32;
        let note_w = (note.duration_ticks as f64 * params.zoom_x) as f32;

        // Y position: higher pitches at top
        let pitch_offset = (max_pitch - note.pitch) as f32 + 1.0;
        let note_y = note_area_y + (pitch_offset / pitch_range) * note_area_h;
        let note_h = (note_area_h / pitch_range).clamp(1.0, 4.0 * params.scale);

        if note_x + note_w >= geom.draw_x && note_x <= geom.draw_end_x {
            canvas.draw_rect(
                vg::Rect::from_xywh(
                    note_x.max(geom.draw_x),
                    note_y,
                    note_w.min(geom.draw_end_x - note_x.max(geom.draw_x)),
                    note_h,
                ),
                &note_paint,
            );
        }
    }

    canvas.restore();
}
