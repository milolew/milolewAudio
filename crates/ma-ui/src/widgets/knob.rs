//! Rotary knob widget for parameter control.
//!
//! Renders a circular arc indicator. Drag up/down to adjust the value.
//! Double-click to reset to the default. Emits `KnobEvent::Changed` on value change.

use vizia::prelude::*;
use vizia::vg;

/// Event emitted by [`Knob`] when its value changes.
#[derive(Debug, Clone)]
pub enum KnobEvent {
    /// Value changed (0.0–1.0 normalized).
    Changed { param_index: usize, value: f32 },
}

/// Rotary knob for a normalized 0.0–1.0 parameter.
pub struct Knob {
    param_index: usize,
    value: f32,
    default_value: f32,
    label: String,
    color: [u8; 3],
    dragging: bool,
    drag_start_y: f32,
    drag_start_value: f32,
}

impl Knob {
    pub fn new(
        cx: &mut Context,
        param_index: usize,
        value: f32,
        default_value: f32,
        label: impl Into<String>,
        color: [u8; 3],
    ) -> Handle<'_, Self> {
        Self {
            param_index,
            value: value.clamp(0.0, 1.0),
            default_value: default_value.clamp(0.0, 1.0),
            label: label.into(),
            color,
            dragging: false,
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        }
        .build(cx, |_cx| {})
    }
}

impl View for Knob {
    fn element(&self) -> Option<&'static str> {
        Some("knob")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();
        let [cr, cg, cb] = self.color;

        let knob_size = bounds.w.min(bounds.h - 14.0 * scale);
        let center_x = bounds.x + bounds.w / 2.0;
        let center_y = bounds.y + knob_size / 2.0 + 2.0 * scale;
        let radius = (knob_size / 2.0 - 2.0 * scale).max(4.0);

        // -- Track arc (background) --
        // Angles in degrees: start at 135° (bottom-left), sweep 270° clockwise
        let start_angle_deg = 135.0_f32;
        let sweep_angle_deg = 270.0_f32;

        let mut track_paint = vg::Paint::default();
        track_paint.set_color(vg::Color::from_argb(80, 80, 80, 80));
        track_paint.set_style(vg::PaintStyle::Stroke);
        track_paint.set_stroke_width(2.5 * scale);
        track_paint.set_anti_alias(true);
        track_paint.set_stroke_cap(vg::paint::Cap::Round);

        let arc_rect = vg::Rect::from_xywh(
            center_x - radius,
            center_y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        let mut track_path = vg::Path::new();
        track_path.add_arc(arc_rect, start_angle_deg, sweep_angle_deg);
        canvas.draw_path(&track_path, &track_paint);

        // -- Value arc (filled portion) --
        let value_sweep_deg = sweep_angle_deg * self.value;
        if value_sweep_deg > 0.1 {
            let mut value_paint = vg::Paint::default();
            value_paint.set_color(vg::Color::from_argb(220, cr, cg, cb));
            value_paint.set_style(vg::PaintStyle::Stroke);
            value_paint.set_stroke_width(2.5 * scale);
            value_paint.set_anti_alias(true);
            value_paint.set_stroke_cap(vg::paint::Cap::Round);

            let mut value_path = vg::Path::new();
            value_path.add_arc(arc_rect, start_angle_deg, value_sweep_deg);
            canvas.draw_path(&value_path, &value_paint);
        }

        // -- Indicator dot --
        let indicator_angle = (start_angle_deg + value_sweep_deg).to_radians();
        let dot_x = center_x + radius * indicator_angle.cos();
        let dot_y = center_y + radius * indicator_angle.sin();
        let dot_radius = 2.0 * scale;

        let mut dot_paint = vg::Paint::default();
        dot_paint.set_color(vg::Color::from_argb(255, 240, 240, 240));
        dot_paint.set_style(vg::PaintStyle::Fill);
        dot_paint.set_anti_alias(true);
        canvas.draw_circle((dot_x, dot_y), dot_radius, &dot_paint);

        // -- Label text below knob --
        let mut label_paint = vg::Paint::default();
        label_paint.set_color(vg::Color::from_argb(180, 200, 200, 200));
        label_paint.set_anti_alias(true);

        let font = vg::Font::default();
        let label_y = bounds.y + bounds.h - 2.0 * scale;

        canvas.save();
        canvas.clip_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            None,
            Some(true),
        );
        canvas.draw_str(
            &self.label,
            (bounds.x + 2.0 * scale, label_y),
            &font,
            &label_paint,
        );
        canvas.restore();
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.dragging = true;
                self.drag_start_y = cx.mouse().cursor_y;
                self.drag_start_value = self.value;
                cx.capture();
                cx.needs_redraw(); // REDRAW: on-change — drag start
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                self.value = self.default_value;
                self.dragging = false;
                cx.emit(KnobEvent::Changed {
                    param_index: self.param_index,
                    value: self.value,
                });
                cx.needs_redraw(); // REDRAW: on-change — reset to default
                meta.consume();
            }
            WindowEvent::MouseMove(_, _) => {
                if self.dragging {
                    let dy = self.drag_start_y - cx.mouse().cursor_y;
                    let sensitivity = cx.bounds().h.max(80.0);
                    let delta = dy / sensitivity;
                    self.value = (self.drag_start_value + delta).clamp(0.0, 1.0);

                    cx.emit(KnobEvent::Changed {
                        param_index: self.param_index,
                        value: self.value,
                    });
                    cx.needs_redraw(); // REDRAW: on-change — knob drag
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.dragging {
                    self.dragging = false;
                    cx.release();
                    meta.consume();
                }
            }
            _ => {}
        });
    }
}
