//! Transport bar widget — play, stop, record, tempo, time display.

use vizia::prelude::*;

use crate::app_data::{AppData, AppEvent};
use crate::types::time::tick_to_bbt;

pub struct TransportBar;

impl TransportBar {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            HStack::new(cx, |cx| {
                // Stop button
                Button::new(cx, |cx| Label::new(cx, "\u{23F9}"))
                    .on_press(|cx| cx.emit(AppEvent::Stop))
                    .class("transport-btn");

                // Play button
                Button::new(cx, |cx| Label::new(cx, "\u{25B6}"))
                    .on_press(|cx| cx.emit(AppEvent::Play))
                    .class("transport-btn")
                    .toggle_class(
                        "playing",
                        AppData::transport.map(|t| t.is_playing && !t.is_recording),
                    );

                // Record button
                Button::new(cx, |cx| Label::new(cx, "\u{23FA}"))
                    .on_press(|cx| cx.emit(AppEvent::Record))
                    .class("transport-btn")
                    .toggle_class("recording", AppData::transport.map(|t| t.is_recording));

                Element::new(cx).class("separator");

                // Position display (Bar.Beat.Tick)
                Label::new(
                    cx,
                    AppData::transport.map(|t| {
                        let bbt = tick_to_bbt(t.position, t.time_signature);
                        format!("{}.{}.{:03}", bbt.bar, bbt.beat, bbt.tick)
                    }),
                )
                .class("position-display");

                Element::new(cx).class("separator");

                // BPM display
                Label::new(cx, "BPM").class("bpm-label");
                Label::new(cx, AppData::transport.map(|t| format!("{:.1}", t.tempo)))
                    .class("tempo-display");

                Element::new(cx).class("separator");

                // Time signature
                Label::new(
                    cx,
                    AppData::transport.map(|t| {
                        format!(
                            "{}/{}",
                            t.time_signature.numerator, t.time_signature.denominator
                        )
                    }),
                )
                .class("time-sig");

                Element::new(cx).class("separator");

                // Loop toggle
                Button::new(cx, |cx| Label::new(cx, "Loop"))
                    .on_press(|cx| cx.emit(AppEvent::ToggleLoop))
                    .class("transport-btn")
                    .toggle_class("active", AppData::transport.map(|t| t.loop_enabled));

                Element::new(cx).class("separator");

                // CPU meter
                Label::new(
                    cx,
                    AppData::mixer.map(|m| format!("CPU: {:.0}%", m.cpu_load * 100.0)),
                )
                .class("cpu-display");
            })
            .class("transport-inner");
        })
    }
}

impl View for TransportBar {
    fn element(&self) -> Option<&'static str> {
        Some("transport-bar")
    }
}
