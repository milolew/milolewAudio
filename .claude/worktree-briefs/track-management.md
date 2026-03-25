# Brief: Track Management

## Scope
- `ma-audio-engine/src/engine.rs`, `command_processor.rs`, nowy `metronome.rs`
- `ma-ui/src/views/arrangement/mod.rs` (track header), `transport_bar.rs`
- ~4 dni

## NIE DOTYKAJ
piano_roll, device_rack, browser, clip_renderer

## Features

1. **Add track runtime** — `engine.add_track(config)` + UI przycisk "+" + Ctrl+T / Ctrl+Shift+T
2. **Remove track** — cleanup clipy/mixer + EngineCommand::RemoveTrack
3. **Rename track** — double-click inline edit
4. **Record arm toggle** — "R" button w track header, czerwone highlight
5. **MetronomeNode** — nowy AudioNode:
   - 1kHz sine click 10ms
   - 1.5kHz accent na beat 1
   - routing do mixer
   - toggle w transport bar
6. **Input monitoring** — MonitorMode enum Off/On/Auto, EngineCommand::SetMonitoring, dropdown w track header
