//! Browser view — file/sample browser panel for loading audio and MIDI files.
//!
//! Displays a directory listing with navigation, filter buttons, and file info.
//! Directories can be entered by clicking. Files can be selected for loading.

use vizia::prelude::*;
use vizia::vg;

use crate::app_data::{AppData, AppEvent};
use crate::state::browser_state::BrowserFilter;

/// Color constants for the browser.
const ENTRY_BG: (u8, u8, u8) = (0x24, 0x24, 0x24);
const ENTRY_SELECTED: (u8, u8, u8) = (0x3A, 0x5A, 0x8A);
const TEXT_COLOR: (u8, u8, u8) = (0xD0, 0xD0, 0xD0);
const DIR_COLOR: (u8, u8, u8) = (0xFF, 0xCC, 0x00);
const AUDIO_COLOR: (u8, u8, u8) = (0x5B, 0x9B, 0xD5);
const MIDI_COLOR: (u8, u8, u8) = (0x80, 0xD0, 0x80);

/// The browser view composing navigation, filter, and file list.
pub struct BrowserView;

impl BrowserView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            // Refresh browser listing on first display
            cx.emit(AppEvent::BrowserRefresh);

            VStack::new(cx, |cx| {
                // -- Navigation bar --
                HStack::new(cx, |cx| {
                    Button::new(cx, |cx| Label::new(cx, "Up"))
                        .on_press(|cx| cx.emit(AppEvent::BrowserGoUp))
                        .class("browser-nav-btn");

                    Button::new(cx, |cx| Label::new(cx, "Refresh"))
                        .on_press(|cx| cx.emit(AppEvent::BrowserRefresh))
                        .class("browser-nav-btn");
                })
                .class("browser-nav-bar");

                // -- Current path display --
                Label::new(
                    cx,
                    AppData::browser.map(|b| {
                        b.current_dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| b.current_dir.to_string_lossy().to_string())
                    }),
                )
                .class("browser-path-label");

                // -- Filter buttons --
                HStack::new(cx, |cx| {
                    Button::new(cx, |cx| Label::new(cx, "All"))
                        .on_press(|cx| cx.emit(AppEvent::BrowserSetFilter(BrowserFilter::All)))
                        .class("browser-filter-btn")
                        .toggle_class(
                            "active",
                            AppData::browser.map(|b| b.filter == BrowserFilter::All),
                        );

                    Button::new(cx, |cx| Label::new(cx, "Audio"))
                        .on_press(|cx| cx.emit(AppEvent::BrowserSetFilter(BrowserFilter::Audio)))
                        .class("browser-filter-btn")
                        .toggle_class(
                            "active",
                            AppData::browser.map(|b| b.filter == BrowserFilter::Audio),
                        );

                    Button::new(cx, |cx| Label::new(cx, "MIDI"))
                        .on_press(|cx| cx.emit(AppEvent::BrowserSetFilter(BrowserFilter::Midi)))
                        .class("browser-filter-btn")
                        .toggle_class(
                            "active",
                            AppData::browser.map(|b| b.filter == BrowserFilter::Midi),
                        );
                })
                .class("browser-filter-bar");

                // -- File list --
                ScrollView::new(cx, |cx| {
                    Binding::new(
                        cx,
                        AppData::browser.map(|b| b.entries.len()),
                        |cx, count_lens| {
                            let count = count_lens.get(cx);

                            for idx in 0..count {
                                BrowserEntryRow::new(cx, idx);
                            }
                        },
                    );
                })
                .class("browser-file-list");
            })
            .class("browser-content");
        })
    }
}

impl View for BrowserView {
    fn element(&self) -> Option<&'static str> {
        Some("browser-view")
    }
}

/// A single row in the browser file list.
struct BrowserEntryRow {
    index: usize,
}

impl BrowserEntryRow {
    fn new(cx: &mut Context, index: usize) -> Handle<'_, Self> {
        Self { index }.build(cx, |_cx| {})
    }
}

impl View for BrowserEntryRow {
    fn element(&self) -> Option<&'static str> {
        Some("browser-entry-row")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let scale = cx.scale_factor();

        let (entry, is_selected) = match cx.data::<AppData>() {
            Some(app) => {
                let entry = app.browser.entries.get(self.index).cloned();
                let selected = app.browser.selected_index == Some(self.index);
                (entry, selected)
            }
            None => (None, false),
        };

        let entry = match entry {
            Some(e) => e,
            None => return,
        };

        // Row background
        let bg_color = if is_selected {
            ENTRY_SELECTED
        } else {
            ENTRY_BG
        };
        let mut bg = vg::Paint::default();
        bg.set_color(vg::Color::from_argb(
            255, bg_color.0, bg_color.1, bg_color.2,
        ));
        bg.set_style(vg::PaintStyle::Fill);
        bg.set_anti_alias(true);
        canvas.draw_rect(
            vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h),
            &bg,
        );

        // Icon/prefix color based on type
        let prefix_color = if entry.is_dir {
            DIR_COLOR
        } else if entry.is_audio() {
            AUDIO_COLOR
        } else if entry.is_midi() {
            MIDI_COLOR
        } else {
            TEXT_COLOR
        };

        let prefix = if entry.is_dir {
            ">"
        } else if entry.is_audio() {
            "~"
        } else if entry.is_midi() {
            "#"
        } else {
            " "
        };

        let font = vg::Font::default();
        let text_y = bounds.y + bounds.h - 4.0 * scale;
        let padding = 6.0 * scale;

        // Type prefix
        let mut prefix_paint = vg::Paint::default();
        prefix_paint.set_color(vg::Color::from_argb(
            255,
            prefix_color.0,
            prefix_color.1,
            prefix_color.2,
        ));
        prefix_paint.set_anti_alias(true);
        canvas.draw_str(prefix, (bounds.x + padding, text_y), &font, &prefix_paint);

        // File name
        let mut text_paint = vg::Paint::default();
        text_paint.set_color(vg::Color::from_argb(
            255,
            TEXT_COLOR.0,
            TEXT_COLOR.1,
            TEXT_COLOR.2,
        ));
        text_paint.set_anti_alias(true);
        canvas.draw_str(
            &entry.name,
            (bounds.x + padding + 14.0 * scale, text_y),
            &font,
            &text_paint,
        );
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                cx.emit(AppEvent::BrowserSelect(self.index));
                meta.consume();
            }
            if let WindowEvent::MouseDoubleClick(MouseButton::Left) = window_event {
                cx.emit(AppEvent::BrowserActivate(self.index));
                meta.consume();
            }
        });

        event.map(|app_event, _meta| {
            if let AppEvent::PollEngine = app_event {
                cx.needs_redraw();
            }
        });
    }
}
