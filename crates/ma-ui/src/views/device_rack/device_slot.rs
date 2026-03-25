//! Device slot — a single effect in the device chain.
//!
//! Renders the device name, bypass toggle, and parameter knobs.
//! Emits `DeviceSlotEvent` when parameters change or bypass is toggled.

use vizia::prelude::*;
use vizia::vg;

use crate::widgets::knob::{Knob, KnobEvent};

/// Events emitted by a device slot.
#[derive(Debug, Clone)]
pub enum DeviceSlotEvent {
    /// A parameter knob was adjusted.
    ParameterChanged {
        device_index: usize,
        param_index: usize,
        value: f32,
    },
    /// Bypass toggled.
    ToggleBypass { device_index: usize },
}

/// Data describing a single parameter.
#[derive(Debug, Clone)]
pub struct ParameterData {
    pub name: String,
    pub value: f32,
    pub default_value: f32,
}

/// Data describing a device slot.
#[derive(Debug, Clone)]
pub struct DeviceSlotData {
    pub name: String,
    pub bypassed: bool,
    pub color: [u8; 3],
    pub parameters: Vec<ParameterData>,
}

/// Renders a single device slot with header, bypass indicator, and parameter knobs.
pub struct DeviceSlot {
    device_index: usize,
    data: DeviceSlotData,
}

impl DeviceSlot {
    pub fn new(cx: &mut Context, device_index: usize, data: DeviceSlotData) -> Handle<'_, Self> {
        let slot = Self {
            device_index,
            data: data.clone(),
        };

        slot.build(cx, |cx| {
            // Parameter knobs row
            for (i, param) in data.parameters.iter().enumerate() {
                Knob::new(
                    cx,
                    i,
                    param.value,
                    param.default_value,
                    &param.name,
                    data.color,
                )
                .width(Pixels(44.0))
                .height(Pixels(56.0));
            }
        })
    }
}

impl View for DeviceSlot {
    fn element(&self) -> Option<&'static str> {
        Some("device-slot")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();
        let [cr, cg, cb] = self.data.color;

        let corner_radius = 4.0 * scale;

        // -- Slot background --
        let bg_alpha = if self.data.bypassed { 20 } else { 40 };
        let mut bg_paint = vg::Paint::default();
        bg_paint.set_color(vg::Color::from_argb(
            255,
            35 + bg_alpha,
            35 + bg_alpha,
            38 + bg_alpha,
        ));
        bg_paint.set_style(vg::PaintStyle::Fill);
        bg_paint.set_anti_alias(true);

        let rect = vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h);
        let rrect = vg::RRect::new_rect_xy(rect, corner_radius, corner_radius);
        canvas.draw_rrect(rrect, &bg_paint);

        // -- Color accent at top --
        let accent_h = 3.0 * scale;
        let accent_alpha = if self.data.bypassed { 60 } else { 200 };
        let mut accent_paint = vg::Paint::default();
        accent_paint.set_color(vg::Color::from_argb(accent_alpha, cr, cg, cb));
        accent_paint.set_style(vg::PaintStyle::Fill);
        accent_paint.set_anti_alias(true);

        let accent_rect = vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, accent_h);
        canvas.save();
        canvas.clip_rrect(rrect, None, Some(true));
        canvas.draw_rect(accent_rect, &accent_paint);
        canvas.restore();

        // -- Device name --
        let text_alpha = if self.data.bypassed { 100 } else { 220 };
        let mut name_paint = vg::Paint::default();
        name_paint.set_color(vg::Color::from_argb(text_alpha, 220, 220, 220));
        name_paint.set_anti_alias(true);

        let font = vg::Font::default();
        let name_y = bounds.y + accent_h + 12.0 * scale;

        canvas.save();
        canvas.clip_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, 20.0 * scale),
            None,
            Some(true),
        );
        canvas.draw_str(
            &self.data.name,
            (bounds.x + 6.0 * scale, name_y),
            &font,
            &name_paint,
        );
        canvas.restore();

        // -- Bypass indicator --
        if self.data.bypassed {
            let mut bypass_paint = vg::Paint::default();
            bypass_paint.set_color(vg::Color::from_argb(140, 200, 80, 80));
            bypass_paint.set_anti_alias(true);

            canvas.draw_str(
                "BYP",
                (bounds.x + bounds.w - 28.0 * scale, name_y),
                &font,
                &bypass_paint,
            );
        }

        // -- Border --
        let mut border_paint = vg::Paint::default();
        border_paint.set_color(vg::Color::from_argb(50, cr, cg, cb));
        border_paint.set_style(vg::PaintStyle::Stroke);
        border_paint.set_stroke_width(0.5 * scale);
        border_paint.set_anti_alias(true);
        canvas.draw_rrect(rrect, &border_paint);
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // Bubble KnobEvent as DeviceSlotEvent with device_index context
        event.map(|knob_event, _meta| {
            let KnobEvent::Changed { param_index, value } = knob_event;
            if let Some(param) = self.data.parameters.get_mut(*param_index) {
                param.value = *value;
            }
            cx.emit(DeviceSlotEvent::ParameterChanged {
                device_index: self.device_index,
                param_index: *param_index,
                value: *value,
            });
        });

        // Double-click on header area toggles bypass
        event.map(|window_event, meta| {
            if let WindowEvent::MouseDoubleClick(MouseButton::Right) = window_event {
                self.data.bypassed = !self.data.bypassed;
                cx.emit(DeviceSlotEvent::ToggleBypass {
                    device_index: self.device_index,
                });
                cx.needs_redraw(); // REDRAW: on-change — bypass toggle
                meta.consume();
            }
        });
    }
}
