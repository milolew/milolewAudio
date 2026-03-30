//! Piano roll state — grid settings, zoom, scroll, and interaction FSM.

use crate::types::midi::NoteId;
use crate::types::time::{QuantizeGrid, Tick};
use crate::types::track::ClipId;

/// Active editing tool in the piano roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PianoRollTool {
    #[default]
    Draw,
    Erase,
    Velocity,
}

/// Which edge of a note is being resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    Left,
    Right,
}

/// Piano roll mouse interaction state machine.
#[derive(Debug, Clone, Default)]
pub enum PianoRollInteraction {
    #[default]
    Idle,
    DrawingNote {
        start_tick: Tick,
        pitch: u8,
        velocity: u8,
        current_end_tick: Tick,
    },
    DraggingNote {
        note_id: NoteId,
        original_start: Tick,
        original_pitch: u8,
        drag_offset_tick: Tick,
        drag_offset_pitch: i8,
    },
    ResizingNote {
        note_id: NoteId,
        original_duration: Tick,
        edge: ResizeEdge,
    },
    Selecting {
        origin_x: f32,
        origin_y: f32,
        current_x: f32,
        current_y: f32,
    },
}

/// Full piano roll state.
#[derive(Debug, Clone)]
pub struct PianoRollState {
    /// Currently open clip (None = piano roll hidden).
    pub active_clip_id: Option<ClipId>,
    /// Horizontal zoom: pixels per tick.
    pub zoom_x: f64,
    /// Vertical zoom: pixels per semitone (note row height).
    pub note_height: f32,
    /// Horizontal scroll in ticks.
    pub scroll_x: f64,
    /// Vertical scroll — top visible MIDI note number.
    pub scroll_y: u8,
    /// Active quantization grid.
    pub quantize: QuantizeGrid,
    /// Current interaction state.
    pub interaction: PianoRollInteraction,
    /// Default velocity for new notes.
    pub default_velocity: u8,
    /// Selected notes.
    pub selected_notes: Vec<NoteId>,
    /// Next note ID counter.
    pub next_note_id: u64,
    /// Active editing tool.
    pub tool: PianoRollTool,
}

impl Default for PianoRollState {
    fn default() -> Self {
        Self {
            active_clip_id: None,
            zoom_x: 0.15,
            note_height: 14.0,
            scroll_x: 0.0,
            scroll_y: 84, // Start around C5
            quantize: QuantizeGrid::Sixteenth,
            interaction: PianoRollInteraction::Idle,
            default_velocity: 100,
            selected_notes: Vec::new(),
            next_note_id: 1,
            tool: PianoRollTool::Draw,
        }
    }
}

impl PianoRollState {
    /// Allocate a new unique NoteId.
    pub fn alloc_note_id(&mut self) -> NoteId {
        let id = NoteId(self.next_note_id);
        self.next_note_id += 1;
        id
    }

    /// Convert tick to x pixel position within the piano roll rect.
    pub fn tick_to_x(&self, tick: Tick) -> f32 {
        ((tick as f64 - self.scroll_x) * self.zoom_x) as f32
    }

    /// Convert x pixel position to tick.
    pub fn x_to_tick(&self, x: f32) -> Tick {
        ((x as f64 / self.zoom_x) + self.scroll_x) as Tick
    }

    /// Convert MIDI note number to y pixel position.
    /// Note 127 is at the top, note 0 at the bottom.
    pub fn pitch_to_y(&self, pitch: u8, rect_top: f32) -> f32 {
        let offset = self.scroll_y as f32 - pitch as f32;
        rect_top + offset * self.note_height
    }

    /// Convert y pixel position to MIDI note number.
    pub fn y_to_pitch(&self, y: f32, rect_top: f32) -> u8 {
        let offset = (y - rect_top) / self.note_height;
        let pitch = self.scroll_y as f32 - offset;
        (pitch.round() as i32).clamp(0, 127) as u8
    }

    /// Number of visible note rows (capped to 128 — the MIDI range).
    pub fn visible_rows(&self, height: f32) -> u8 {
        let rows = (height / self.note_height).ceil() as u32;
        rows.min(128) as u8
    }
}
