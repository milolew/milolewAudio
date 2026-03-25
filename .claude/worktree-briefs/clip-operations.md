# Brief: Clip Operations

## Scope
- `ma-ui/src/views/arrangement/` + nowe pliki
- ~5 dni

## NIE DOTYKAJ
ma-core, ma-audio-engine, piano_roll, device_rack, mixer, browser

## Nowe pliki
- `clip_interaction.rs` — mouse FSM: Idle / RubberBand / DraggingClip / ResizingLeft / ResizingRight
- `snap.rs` — grid snap logic
- `clipboard.rs` — copy/paste buffer
- `selection.rs` — SelectionState z HashSet<ClipId>

## Features (w kolejności implementacji)

1. **Selection system** — click / shift+click / rubber band select
2. **Snap grid** — dropdown w toolbar: Off / Bar / Half / Quarter / Eighth / Sixteenth + `snap_tick()` function
3. **Move clip drag** — ghost render 50% opacity, snap, multi-select moves all
4. **Resize clip trim** — krawędź 6px, cursor ew-resize, audio=offset, MIDI=trim notes
5. **Split** Ctrl+E — playhead wewnątrz clipu → dwa nowe clipy
6. **Duplicate** Ctrl+D — deep clone, umieść za oryginałem
7. **Delete** key — usuń z ProjectState + EngineCommand
8. **Copy/Paste** Ctrl+C/V — clipboard Vec<ClipState>, paste na playhead

## Testy
- snap boundary cases
- move verify position
- split verify durations
- selection toggle
