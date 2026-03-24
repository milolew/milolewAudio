//! Preferences view — audio device selection and configuration.

use vizia::prelude::*;

use crate::app_data::{AppData, AppEvent};

pub struct PreferencesView;

impl PreferencesView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            Label::new(cx, "Audio Preferences").class("prefs-title");

            VStack::new(cx, |cx| {
                // Output device
                HStack::new(cx, |cx| {
                    Label::new(cx, "Output Device:").class("prefs-label");
                    Label::new(cx, AppData::device_status_text.map(|s| s.clone()))
                        .class("prefs-value");
                })
                .class("prefs-row");

                // Sample rate
                HStack::new(cx, |cx| {
                    Label::new(cx, "Sample Rate:").class("prefs-label");
                    Label::new(cx, AppData::device_sample_rate.map(|s| s.clone()))
                        .class("prefs-value");
                })
                .class("prefs-row");

                // Buffer size
                HStack::new(cx, |cx| {
                    Label::new(cx, "Buffer Size:").class("prefs-label");
                    Label::new(cx, AppData::device_buffer_size.map(|s| s.clone()))
                        .class("prefs-value");
                })
                .class("prefs-row");

                // Latency
                HStack::new(cx, |cx| {
                    Label::new(cx, "Latency:").class("prefs-label");
                    Label::new(cx, AppData::device_latency.map(|s| s.clone())).class("prefs-value");
                })
                .class("prefs-row");

                // Refresh button
                Button::new(cx, |cx| Label::new(cx, "Refresh Devices"))
                    .on_press(|cx| cx.emit(AppEvent::RefreshDevices))
                    .class("prefs-btn");
            })
            .class("prefs-content");
        })
    }
}

impl View for PreferencesView {
    fn element(&self) -> Option<&'static str> {
        Some("preferences-view")
    }
}
