# Brief: Recording Flow

## Scope
- `ma-ui/src/views/arrangement/mod.rs` (loop/scroll)
- `ma-ui/src/app_data/dispatch.rs` (record events)
- `ma-audio-engine/` (monitoring routing)
- ~4 dni

## START: po merge feat/track-management

## NIE DOTYKAJ
piano_roll, device_rack, browser

## Features

1. **Record button flow** — check armed → start → stop → RecordingComplete → clip appears
2. **Recording→clip creation** — ClipState at record_start_position + peak cache build
3. **Loop region drag** — Shift+drag na ruler, żółte markery, resize, L toggle
4. **Follow playhead** — scroll gdy >80% viewport, auto-disable na manual scroll
5. **Zoom/Scroll**:
   - Ctrl+Scroll — zoom horizontal
   - Shift+Scroll — scroll horizontal
   - Scroll — scroll vertical
   - +/- — zoom
6. **Input monitoring wiring** — route live input through effects to output
