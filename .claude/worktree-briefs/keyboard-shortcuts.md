# Brief: Keyboard Shortcuts

## Scope
- `ma-ui/src/keyboard.rs` (nowy plik)
- `ma-ui/src/views/piano_roll_view.rs` (quantize/transpose)
- ~3 dni

## START: po merge feat/undo-system + feat/clip-operations

## NIE DOTYKAJ
ma-audio-engine, arrangement/clip_renderer, browser, device_rack

## Centralny handler
`keyboard.rs` przechwytuje `WindowEvent::KeyDown`.

### Transport shortcuts
- Space — play/pause
- R — record
- Home — go to start
- L — loop toggle
- M — metronome toggle
- F — follow playhead toggle

### Edit shortcuts
- Ctrl+Z — undo
- Ctrl+Shift+Z — redo
- Ctrl+S — save
- Ctrl+T — add audio track
- Ctrl+Shift+T — add MIDI track
- Delete — delete selected
- Ctrl+C — copy
- Ctrl+V — paste
- Ctrl+D — duplicate
- Ctrl+E — split at playhead
- Ctrl+A — select all

### Navigation
- +/- — zoom in/out
- Ctrl+Scroll — horizontal zoom

## Piano roll specifics
- Ctrl+Q — quantize selected notes
- ↑↓ — transpose ±1 semitone
- Shift+↑↓ — transpose ±12 (octave)
- Ctrl+A — select all notes
- V/D/E — tool switch (velocity/draw/erase)
- Velocity drag w velocity lane

## Quantize function
Snap notes do grid z strength parameter (0.0–1.0).
