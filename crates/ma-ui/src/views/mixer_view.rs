//! Mixer View — horizontal row of channel strips with faders, meters, mute/solo.

use vizia::prelude::*;

pub struct MixerView;

impl MixerView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            Label::new(cx, "Mixer View").class("placeholder");
        })
    }
}

impl View for MixerView {
    fn element(&self) -> Option<&'static str> {
        Some("mixer-view")
    }
}
