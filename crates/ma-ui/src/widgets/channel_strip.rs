//! Channel strip widget — combines fader, meter, pan, mute/solo for one track.
//!
//! Composes a PeakMeter, Fader, mute/solo buttons, track name label,
//! and volume dB readout into a single vertical strip for the mixer view.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::types::track::TrackId;
use crate::widgets::fader::Fader;
use crate::widgets::pan_knob::PanKnob;
use crate::widgets::peak_meter::PeakMeter;

/// Color accent bar drawn at the left edge of the channel strip.
/// Uses the track's color from AppData.
struct ColorBar {
    track_id: TrackId,
}

impl ColorBar {
    fn new(cx: &mut Context, track_id: TrackId) -> Handle<'_, Self> {
        Self { track_id }.build(cx, |_cx| {})
    }
}

impl View for ColorBar {
    fn element(&self) -> Option<&'static str> {
        Some("strip-color-bar")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();

        let color = cx
            .data::<AppData>()
            .and_then(|app| app.track(self.track_id))
            .map(|t| t.color)
            .unwrap_or([128, 128, 128]);

        let mut paint = vg::Paint::default();
        paint.set_color(vg::Color::from_argb(255, color[0], color[1], color[2]));
        paint.set_style(vg::PaintStyle::Fill);
        paint.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &paint,
        );
    }
}

/// A single mixer channel strip for one track.
pub struct ChannelStrip {
    _track_id: TrackId,
}

impl ChannelStrip {
    pub fn new(cx: &mut Context, track_id: TrackId) -> Handle<'_, Self> {
        Self {
            _track_id: track_id,
        }
        .build(cx, |cx| {
            HStack::new(cx, |cx| {
                // Color accent bar at the left edge (4px wide)
                ColorBar::new(cx, track_id).width(Pixels(4.0));

                VStack::new(cx, |cx| {
                    // -- Track name label --
                    Label::new(
                        cx,
                        AppData::tracks.map(move |tracks| {
                            tracks
                                .iter()
                                .find(|t| t.id == track_id)
                                .map(|t| t.name.clone())
                                .unwrap_or_default()
                        }),
                    )
                    .class("strip-name");

                    // -- Mute / Solo buttons --
                    HStack::new(cx, |cx| {
                        let mute_track_id = track_id;
                        Button::new(cx, |cx| Label::new(cx, "M"))
                            .on_press(move |cx| cx.emit(AppEvent::ToggleMute(mute_track_id)))
                            .class("strip-mute-btn")
                            .toggle_class(
                                "active",
                                AppData::tracks.map(move |tracks| {
                                    tracks
                                        .iter()
                                        .find(|t| t.id == mute_track_id)
                                        .map(|t| t.mute)
                                        .unwrap_or(false)
                                }),
                            );

                        let solo_track_id = track_id;
                        Button::new(cx, |cx| Label::new(cx, "S"))
                            .on_press(move |cx| cx.emit(AppEvent::ToggleSolo(solo_track_id)))
                            .class("strip-solo-btn")
                            .toggle_class(
                                "active",
                                AppData::tracks.map(move |tracks| {
                                    tracks
                                        .iter()
                                        .find(|t| t.id == solo_track_id)
                                        .map(|t| t.solo)
                                        .unwrap_or(false)
                                }),
                            );
                    })
                    .class("strip-btn-row");

                    // -- Pan knob --
                    PanKnob::new(cx, track_id)
                        .class("strip-pan-knob")
                        .height(Pixels(44.0))
                        .width(Pixels(44.0));

                    // -- Fader + PeakMeter side by side --
                    HStack::new(cx, |cx| {
                        Fader::new(cx, track_id).class("strip-fader");
                        PeakMeter::new(cx, track_id).class("strip-meter");
                    })
                    .class("strip-fader-meter");

                    // -- Volume dB readout --
                    Label::new(
                        cx,
                        AppData::tracks.map(move |tracks| {
                            let volume = tracks
                                .iter()
                                .find(|t| t.id == track_id)
                                .map(|t| t.volume)
                                .unwrap_or(0.0);
                            if volume <= 0.0 {
                                "-inf dB".to_string()
                            } else {
                                format!("{:.1} dB", 20.0 * volume.log10())
                            }
                        }),
                    )
                    .class("strip-db-label");
                })
                .class("strip-content");
            });
        })
    }
}

impl View for ChannelStrip {
    fn element(&self) -> Option<&'static str> {
        Some("channel-strip")
    }
}
