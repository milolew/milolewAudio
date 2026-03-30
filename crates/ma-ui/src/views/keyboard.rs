//! Centralized keyboard shortcut handler.
//!
//! Called from `RootView::event()` to intercept `WindowEvent::KeyDown`
//! before child views see it. Dispatches `AppEvent` variants based on
//! the active view context.

use vizia::prelude::*;

use crate::app_data::{ActiveView, AppData, AppEvent};
use crate::state::piano_roll_state::PianoRollTool;
use crate::types::time::Tick;

/// Process a vizia event for keyboard shortcuts.
/// Must be called from the root view's `event()` method.
pub fn handle_key_event(cx: &mut EventContext, event: &mut Event) {
    event.map(|window_event, meta| {
        if let WindowEvent::KeyDown(code, _) = window_event {
            let modifiers = cx.modifiers();
            let ctrl = modifiers.contains(Modifiers::CTRL);
            let shift = modifiers.contains(Modifiers::SHIFT);

            let Some(app) = cx.data::<AppData>() else {
                return;
            };
            let active_view = app.active_view;

            // Context-specific shortcuts first
            let handled = match active_view {
                ActiveView::PianoRoll => handle_piano_roll_keys(cx, code, ctrl, shift),
                ActiveView::Arrangement => handle_arrangement_keys(cx, code, ctrl, shift),
                _ => false,
            };

            if handled {
                meta.consume();
                return;
            }

            // Global shortcuts
            if handle_global_keys(cx, code, ctrl, shift, active_view) {
                meta.consume();
            }
        }
    });
}

fn handle_piano_roll_keys(cx: &mut EventContext, code: &Code, ctrl: bool, shift: bool) -> bool {
    match code {
        Code::KeyQ if ctrl => {
            cx.emit(AppEvent::QuantizeSelectedNotes);
            true
        }
        Code::ArrowUp if shift => {
            cx.emit(AppEvent::TransposeSelectedNotes { semitones: 12 });
            true
        }
        Code::ArrowDown if shift => {
            cx.emit(AppEvent::TransposeSelectedNotes { semitones: -12 });
            true
        }
        Code::ArrowUp if !ctrl && !shift => {
            cx.emit(AppEvent::TransposeSelectedNotes { semitones: 1 });
            true
        }
        Code::ArrowDown if !ctrl && !shift => {
            cx.emit(AppEvent::TransposeSelectedNotes { semitones: -1 });
            true
        }
        Code::KeyV if !ctrl => {
            cx.emit(AppEvent::SetPianoRollTool(PianoRollTool::Velocity));
            true
        }
        Code::KeyD if !ctrl => {
            cx.emit(AppEvent::SetPianoRollTool(PianoRollTool::Draw));
            true
        }
        Code::KeyE if !ctrl => {
            cx.emit(AppEvent::SetPianoRollTool(PianoRollTool::Erase));
            true
        }
        Code::KeyA if ctrl => {
            cx.emit(AppEvent::SelectAllNotes);
            true
        }
        Code::Delete | Code::Backspace if !ctrl => {
            cx.emit(AppEvent::DeleteSelectedNotes);
            true
        }
        _ => false,
    }
}

fn handle_arrangement_keys(cx: &mut EventContext, code: &Code, ctrl: bool, _shift: bool) -> bool {
    match code {
        Code::KeyE if ctrl => {
            cx.emit(AppEvent::SplitClipAtPlayhead);
            true
        }
        Code::KeyD if ctrl => {
            cx.emit(AppEvent::DuplicateSelectedClips);
            true
        }
        Code::KeyC if ctrl => {
            cx.emit(AppEvent::CopySelectedClips);
            true
        }
        Code::KeyV if ctrl => {
            cx.emit(AppEvent::PasteClips);
            true
        }
        Code::KeyA if ctrl => {
            cx.emit(AppEvent::SelectAllClips);
            true
        }
        Code::Delete | Code::Backspace if !ctrl => {
            cx.emit(AppEvent::DeleteSelectedClips);
            true
        }
        _ => false,
    }
}

fn handle_global_keys(
    cx: &mut EventContext,
    code: &Code,
    ctrl: bool,
    shift: bool,
    active_view: ActiveView,
) -> bool {
    match code {
        // Transport
        Code::Space => {
            cx.emit(AppEvent::TogglePlayPause);
            true
        }
        Code::KeyR if !ctrl => {
            cx.emit(AppEvent::Record);
            true
        }
        Code::Home => {
            cx.emit(AppEvent::SetPosition(0 as Tick));
            true
        }
        Code::KeyL if !ctrl => {
            cx.emit(AppEvent::ToggleLoop);
            true
        }
        Code::KeyM if !ctrl => {
            cx.emit(AppEvent::ToggleMetronome);
            true
        }
        Code::KeyF if !ctrl => {
            cx.emit(AppEvent::ToggleFollowPlayhead);
            true
        }

        // Undo/Redo
        Code::KeyZ if ctrl && shift => {
            cx.emit(AppEvent::Redo);
            true
        }
        Code::KeyZ if ctrl && !shift => {
            cx.emit(AppEvent::Undo);
            true
        }

        // Save
        Code::KeyS if ctrl => {
            cx.emit(AppEvent::SaveCurrentProject);
            true
        }

        // Track management
        Code::KeyT if ctrl && shift => {
            cx.emit(AppEvent::AddMidiTrack);
            true
        }
        Code::KeyT if ctrl && !shift => {
            cx.emit(AppEvent::AddAudioTrack);
            true
        }

        // Zoom
        Code::Equal | Code::NumpadAdd => {
            let event = match active_view {
                ActiveView::PianoRoll => AppEvent::ZoomPianoRoll(1.2),
                _ => AppEvent::ZoomArrangement(1.2),
            };
            cx.emit(event);
            true
        }
        Code::Minus | Code::NumpadSubtract => {
            let event = match active_view {
                ActiveView::PianoRoll => AppEvent::ZoomPianoRoll(0.8),
                _ => AppEvent::ZoomArrangement(0.8),
            };
            cx.emit(event);
            true
        }

        _ => false,
    }
}
