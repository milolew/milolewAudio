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

                // BPM display — Binding avoids format! on every transport poll
                Label::new(cx, "BPM").class("bpm-label");
                Binding::new(
                    cx,
                    AppData::transport.map(|t| (t.tempo * 10.0).round() as i64),
                    |cx, tempo_key| {
                        let raw = tempo_key.get(cx) as f64 / 10.0;
                        Label::new(cx, &format!("{raw:.1}")).class("tempo-display");
                    },
                );

                Element::new(cx).class("separator");

                // Time signature — Binding avoids format! on every transport poll
                Binding::new(
                    cx,
                    AppData::transport
                        .map(|t| (t.time_signature.numerator, t.time_signature.denominator)),
                    |cx, ts_key| {
                        let (num, den) = ts_key.get(cx);
                        Label::new(cx, &format!("{num}/{den}")).class("time-sig");
                    },
                );

                Element::new(cx).class("separator");

                // Loop toggle
                Button::new(cx, |cx| Label::new(cx, "Loop"))
                    .on_press(|cx| cx.emit(AppEvent::ToggleLoop))
                    .class("transport-btn")
                    .toggle_class("active", AppData::transport.map(|t| t.loop_enabled));

                // Metronome toggle
                Button::new(cx, |cx| Label::new(cx, "Click"))
                    .on_press(|cx| cx.emit(AppEvent::ToggleMetronome))
                    .class("transport-btn")
                    .toggle_class("active", AppData::transport.map(|t| t.metronome_enabled));

                Element::new(cx).class("separator");

                // CPU meter — Binding avoids format! on every mixer poll
                Binding::new(
                    cx,
                    AppData::mixer.map(|m| (m.cpu_load * 100.0).round() as i32),
                    |cx, cpu_key| {
                        let pct = cpu_key.get(cx);
                        Label::new(cx, &format!("CPU: {pct}%")).class("cpu-display");
                    },
                );
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
