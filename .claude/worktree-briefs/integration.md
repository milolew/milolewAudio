# Brief: feat/integration

## Scope
Nowe pliki we WSZYSTKICH crate'ach. Minimalne zmiany w istniejących.
3 feature'y. ~5 dni.
START: po merge fix/engine-safety + fix/ui-architecture do main.

## Feature 1: MIDI Record & Playback (dzień 1-2)
- `ma-core/src/midi_clip.rs` — MidiClip, MidiNote structs
- `ma-audio-engine/src/graph/nodes/midi_player.rs` — playback node
- `ma-audio-engine/src/midi_recorder.rs` — recording z note pairing
- Integracja: piano roll → AudioCommand::LoadMidiClip

## Feature 2: Full Mixer View (dzień 3-4)
- `ma-ui/src/views/mixer/mod.rs` — MixerView
- `ma-ui/src/views/mixer/channel_strip.rs` — fader + meter + pan + sends
- `ma-ui/src/views/mixer/master_strip.rs` — master channel

## Feature 3: File Browser (dzień 4-5)
- `ma-ui/src/views/browser/mod.rs` — BrowserView
- `ma-ui/src/views/browser/file_tree.rs` — directory tree
- `ma-ui/src/views/browser/preview.rs` — audio preview player
- Drag & drop: file → arrangement = import clip

## Po zakończeniu
```bash
git push -u origin feat/integration
gh pr create --title "feat: MIDI record/playback, mixer view, file browser" --base main
```
