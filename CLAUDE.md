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
Jeśli pracujesz w worktree, dotykaj WYŁĄCZNIE swoich plików:

| Worktree branch | Scope plików | Nie dotykaj |
|---|---|---|
| `fix/engine-safety` | `crates/ma-audio-engine/**` | ma-core, ma-ui |
| `fix/ui-architecture` | `crates/ma-ui/**` POZA views/arrangement_view.rs | ma-core, ma-audio-engine, arrangement_view.rs |
| `feat/ui-views` | `crates/ma-ui/src/views/**`, `widgets/**` | ma-core, ma-audio-engine, app_data.rs, bridge/ |
| `feat/integration` | nowe pliki we WSZYSTKICH crate'ach | istniejące pliki — minimalne zmiany |

Jeśli musisz zmienić publiczne API innego crate'a (np. dodać wariant do enum w ma-core) — dodaj go z `#[non_exhaustive]` i zadokumentuj w commit message.

## Auto-brief
Jeśli pracujesz w worktree, przeczytaj swój brief na starcie sesji:
```bash
cat .claude/worktree-briefs/$(git branch --show-current | sed 's|.*/||').md
```
Jeśli plik nie istnieje, zapytaj użytkownika o zakres pracy.

## Aktywne issues (19)
### CRITICAL
- E1: topology.rs — unsafe ptr arithmetic z debug_assert bounds
- E2: 31 sites — Relaxed atomic ordering na cross-thread state

### HIGH
- E3: callback.rs — 8x silent ring buffer overflow (brak counter)
- E4: topology.rs — cycle detection zwraca partial schedule
- E6: disk_io.rs — silent WAV truncation na disk error
- E7: command_processor, disk_io, device_manager — zero testów
- N1: callback.rs — brak catch_unwind na audio thread
- N2: device_manager.rs — cpal errors nie propagowane do UI
- U1: app_data.rs (765 linii) — god object
- U5: arrangement_view.rs — 281-liniowy monolityczny draw
- U6: views/, widgets/ — zero test coverage

### MEDIUM
- E5: input_capture.rs — brak overflow counter
- E8: recording path — brak E2E test
- U4: app_data.rs — O(n) Vec lookup 60fps
- U7: transport_bar.rs — format!() w render loop
- U8: engine.rs — ring buffer hard-coded, brak monitoring
- U12: app_data.rs — 115 linii inline demo data

### LOW
- U10: inconsistent cx.needs_redraw() patterns
- U11: dead code clips_for_track()
