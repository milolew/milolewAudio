//! Mixer View — horizontal row of channel strips with faders, meters, mute/solo.
//!
//! Displays one ChannelStrip per track in a horizontal layout,
//! plus a CPU load indicator at the trailing edge.

use vizia::prelude::*;

use crate::app_data::AppData;
use crate::widgets::channel_strip::ChannelStrip;

/// Mixer view composing channel strips for all project tracks.
pub struct MixerView;

impl MixerView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            HStack::new(cx, |cx| {
                // Reactive binding: rebuild strips when track count changes.
                Binding::new(
                    cx,
                    AppData::tracks.map(|tracks| tracks.len()),
                    |cx, count_lens| {
                        let count = count_lens.get(cx);

                        // Resolve concrete TrackIds via the tracks lens.
                        let tracks = AppData::tracks.get(cx);
                        for idx in 0..count {
                            if let Some(track) = tracks.get(idx) {
                                ChannelStrip::new(cx, track.id);
                            }
                        }
                    },
                );

                // CPU load indicator — Binding avoids format! on every mixer poll
                VStack::new(cx, |cx| {
                    Label::new(cx, "CPU").class("cpu-title");
                    Binding::new(
                        cx,
                        AppData::mixer.map(|m| (m.cpu_load * 100.0).round() as i32),
                        |cx, cpu_key| {
                            let pct = cpu_key.get(cx);
                            Label::new(cx, &format!("{pct}%")).class("cpu-value");
                        },
                    );
                })
                .class("mixer-cpu-panel");
            })
            .class("mixer-strip-row");
        })
    }
}

impl View for MixerView {
    fn element(&self) -> Option<&'static str> {
        Some("mixer-view")
    }
}
