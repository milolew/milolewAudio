//! Root view — main application layout with tab bar and view switching.

use vizia::prelude::*;

use crate::app_data::{ActiveView, AppData, AppEvent};
use crate::types::track::TrackKind;
use crate::views::arrangement::ArrangementView;
use crate::views::browser_view::BrowserView;
use crate::views::mixer_view::MixerView;
use crate::views::piano_roll_view::PianoRollView;
use crate::widgets::transport_bar::TransportBar;

pub struct RootView;

impl RootView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            // Kick off engine polling timer
            cx.emit(AppEvent::InitTimer);

            // Control bar at top
            TransportBar::new(cx);

            // Tab bar
            HStack::new(cx, |cx| {
                Button::new(cx, |cx| Label::new(cx, "Arrangement"))
                    .on_press(|cx| cx.emit(AppEvent::SwitchView(ActiveView::Arrangement)))
                    .class("view-tab")
                    .toggle_class(
                        "active",
                        AppData::active_view.map(|v| *v == ActiveView::Arrangement),
                    );

                Button::new(cx, |cx| Label::new(cx, "Mixer"))
                    .on_press(|cx| cx.emit(AppEvent::SwitchView(ActiveView::Mixer)))
                    .class("view-tab")
                    .toggle_class(
                        "active",
                        AppData::active_view.map(|v| *v == ActiveView::Mixer),
                    );

                Button::new(cx, |cx| Label::new(cx, "Piano Roll"))
                    .on_press(|cx| cx.emit(AppEvent::SwitchView(ActiveView::PianoRoll)))
                    .class("view-tab")
                    .toggle_class(
                        "active",
                        AppData::active_view.map(|v| *v == ActiveView::PianoRoll),
                    );

                Button::new(cx, |cx| Label::new(cx, "Browser"))
                    .on_press(|cx| cx.emit(AppEvent::ToggleBrowser))
                    .class("view-tab")
                    .toggle_class(
                        "active",
                        AppData::active_view.map(|v| *v == ActiveView::Browser),
                    );

                // Show active clip name in piano roll mode
                Label::new(
                    cx,
                    AppData::active_view.map(|v| {
                        if *v == ActiveView::PianoRoll {
                            "Piano Roll".to_string()
                        } else {
                            String::new()
                        }
                    }),
                )
                .class("editing-label");
            })
            .class("tab-bar");

            // Main content area — switches based on active_view
            Binding::new(cx, AppData::active_view, |cx, view| match view.get(cx) {
                ActiveView::Arrangement => {
                    ArrangementView::new(cx);
                }
                ActiveView::Mixer => {
                    MixerView::new(cx);
                }
                ActiveView::PianoRoll => {
                    PianoRollView::new(cx);
                }
                ActiveView::Browser => {
                    BrowserView::new(cx);
                }
            });
        })
    }
}

impl View for RootView {
    fn element(&self) -> Option<&'static str> {
        Some("root-view")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            if let WindowEvent::KeyDown(code, _) = window_event {
                let modifiers = cx.modifiers();
                if modifiers.contains(Modifiers::CTRL) && code == &Code::KeyT {
                    // Ctrl+T → add audio track, Ctrl+Shift+T → add MIDI track
                    let kind = if modifiers.contains(Modifiers::SHIFT) {
                        TrackKind::Midi
                    } else {
                        TrackKind::Audio
                    };
                    cx.emit(AppEvent::AddTrack(kind));
                    meta.consume();
                }
            }
        });
    }
}
