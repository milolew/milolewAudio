//! Channel strip widget — combines fader, meter, pan, mute/solo for one track.

use vizia::prelude::*;

pub struct ChannelStrip;

impl ChannelStrip {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |_cx| {})
    }
}

impl View for ChannelStrip {
    fn element(&self) -> Option<&'static str> {
        Some("channel-strip")
    }
}
