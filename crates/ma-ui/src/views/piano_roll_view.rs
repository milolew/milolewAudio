//! Piano Roll View — MIDI note editor with grid, keyboard strip, and mouse interaction FSM.

use vizia::prelude::*;

pub struct PianoRollView;

impl PianoRollView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            Label::new(cx, "Piano Roll View").class("placeholder");
        })
    }
}

impl View for PianoRollView {
    fn element(&self) -> Option<&'static str> {
        Some("piano-roll-view")
    }
}
