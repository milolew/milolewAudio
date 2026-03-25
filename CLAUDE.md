# milolew Audio — DAW w Ruście

## Architektura
3 crate'y w `crates/`:
- `ma-core` — typy, time, MIDI newtypes, audio buffer, commands/events
- `ma-audio-engine` — audio graph, transport, recording, disk I/O, device manager
- `ma-ui` — vizia GUI: views, widgets, app state, engine bridge

## Zasady commitów
- `fix(core):` / `fix(engine):` / `fix(ui):` — bugfix
- `feat(core):` / `feat(engine):` / `feat(ui):` — nowy feature
- `refactor(ui):` — refaktor bez zmiany zachowania
- `test(engine):` / `test(ui):` — nowe testy
- `ci:` / `docs:` — infrastruktura

## Przed KAŻDYM commitem (obowiązkowe)
```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
```
Jeśli cokolwiek nie przechodzi — napraw ZANIM commitujesz.

## Audio thread safety — BEZWZGLĘDNE ZASADY
- ZERO alokacji w audio callback (no Vec::push at capacity, no format!, no Box::new, no String, no println!)
- Dropping Box/Vec/String/Arc w audio thread → użyj basedrop lub przenieś drop na inny wątek
- Atomiki: `Release` na store, `Acquire` na load dla stanu cross-thread (transport position, is_recording). `Relaxed` OK TYLKO dla single-value eventual consistency (volume, pan, mute, solo)
- Ring buffer overflow: inkrementuj atomic counter, NIGDY nie loguj z RT thread
- `assert_no_alloc` w debug buildach na audio callback

## Ownership plików per worktree

| Worktree branch | Scope plików | Nie dotykaj |
|---|---|---|
| `feat/undo-system` | `ma-core/src/undo.rs` (nowy) + `ma-ui/src/app_data/` | views/, widgets/, ma-audio-engine |
| `feat/clip-operations` | `ma-ui/src/views/arrangement/` + nowe pliki clip_interaction.rs, snap.rs, clipboard.rs | ma-core, ma-audio-engine, piano_roll, device_rack, mixer, browser |
| `feat/track-management` | `ma-audio-engine/src/engine.rs`, `command_processor.rs`, nowy `metronome.rs` + `ma-ui/src/views/arrangement/mod.rs` (track header), `transport_bar.rs` | piano_roll, device_rack, browser, clip_renderer |
| `feat/keyboard-shortcuts` | `ma-ui/src/keyboard.rs` (nowy) + `ma-ui/src/views/piano_roll_view.rs` (quantize/transpose) | ma-audio-engine, arrangement/clip_renderer, browser, device_rack |
| `feat/recording-flow` | `ma-ui/src/views/arrangement/mod.rs` (loop/scroll) + `ma-ui/src/app_data/dispatch.rs` (record events) + `ma-audio-engine/` (monitoring routing) | piano_roll, device_rack, browser |
| `feat/polish` | cross-crate — QoL features | — (final cleanup) |

Jeśli musisz zmienić publiczne API innego crate'a (np. dodać wariant do enum w ma-core) — dodaj go z `#[non_exhaustive]` i zadokumentuj w commit message.

## Auto-brief
Jeśli pracujesz w worktree, przeczytaj swój brief na starcie sesji:
```bash
cat .claude/worktree-briefs/$(git branch --show-current | sed 's|.*/||').md
```
Jeśli plik nie istnieje, zapytaj użytkownika o zakres pracy.

## Aktywne issues (8) + Feature gaps (37)

### Issues (tech debt)
- U1 HIGH: app_data.rs (1345 linii) — god object
- U6 HIGH: most views/widgets — partial test coverage
- E8 MEDIUM: recording overflow — no E2E test
- U4 MEDIUM: O(n) Vec lookup 60fps
- U8 MEDIUM: ring buffer unmonitored
- U12 MEDIUM: 133 linii inline demo data
- U10 LOW: inconsistent redraw patterns
- U11 LOW: dead code clips_for_track()

### Feature gaps P0 (BLOCKER)
- Undo/Redo system — brak jakiegokolwiek undo

### Feature gaps P1 (core workflow — 35 items)
- Clip operations: move, resize, split, duplicate, delete, copy/paste, multi-select
- Track management: add/remove/rename runtime, record arm GUI
- Arrangement: snap grid, selection system, rubber band select
- Recording: arm+record→clip appears, loop region drag, follow playhead, input monitoring
- Piano roll: quantize, transpose, velocity drag, select all, snap selector
- Metronome click audio
- Keyboard shortcuts: Space, R, Ctrl+Z, Ctrl+S, Ctrl+E, Ctrl+D, Delete, Ctrl+T, +/-
- Mute/Solo sync z engine

### Resolved (20 issues cumulative)
E1, E2, E3, E4, E5, E6, E7, N1, N2 (bac607f) | U5 (58bd185) | U7 (4a43bdc) | C1-C6, U2, U3, U9 (prior)
