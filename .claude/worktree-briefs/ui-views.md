# Brief: feat/ui-views

## Scope
`crates/ma-ui/src/views/` i `widgets/`. 4 issues + 2 nowe views. ~5 dni.
START: po merge fix/engine-safety + fix/ui-architecture do main.

## Krok 1: U5 — Decompose arrangement_view.rs (dzień 1-2)
Rozbij `TrackLane::draw()` (281 linii) na:
- `views/arrangement/grid.rs` — draw_grid(), draw_ruler()
- `views/arrangement/clip_renderer.rs` — draw_audio_clip(), draw_midi_clip()
- `views/arrangement/playhead.rs` — draw_playhead(), draw_loop_region()

## Krok 2: U7 + U10 — performance + consistency (dzień 2)
- U7: Cache position string w transport_bar (format! tylko gdy wartość się zmieni)
- U10: Standaryzuj `cx.needs_redraw()` — animated vs on-change

## Krok 3: Live waveform display (dzień 3)
- `views/arrangement/live_waveform.rs` — rosnący waveform podczas nagrywania
- Pobieraj peak data z AudioFeedback.recording_peaks co frame

## Krok 4: Device Rack View (dzień 4-5)
- `views/device_rack/mod.rs` — łańcuch efektów L→R
- `views/device_rack/device_slot.rs` — slot z knobami parametrów
- Integracja: knob change → AudioCommand::SetParameter

## Krok 5: U6 — testy views (dzień 5)
- Coordinate conversion: pixel↔tick, pixel↔pitch
- Hit testing: klik na clip, klik na nutę

## Po zakończeniu
```bash
git push -u origin feat/ui-views
gh pr create --title "feat(ui): arrangement decompose, live waveform, device rack" --base main
```
