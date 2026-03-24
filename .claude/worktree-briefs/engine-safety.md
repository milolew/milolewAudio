# Brief: fix/engine-safety

## Scope
WYŁĄCZNIE `crates/ma-audio-engine/`. 10 issues. ~5 dni.

## Batch 1: Safety-Critical (dzień 1-2)

### E1 [CRITICAL] — topology.rs:125-183
Unsafe `buf_base.add(idx)` z `debug_assert`-only bounds check.
Silent truncation `.min(MAX_NODE_IO)` na >16 IO per node.

**Fix:**
- Zamień raw pointer arithmetic na slice-based access z checked bounds
- `.min(MAX_NODE_IO)` → `if count > MAX_NODE_IO { return Err(TopologyError::TooManyIO { node_id, count }) }`
- Dodaj safety comment lub zamień na safe Rust
- Zachowaj zero-alloc — nie używaj Vec w hot path

### E2 [CRITICAL] — 31 sites, 6 files — atomic ordering
13 z 31 sites wymaga silniejszego ordering (patrz tabela).

**Zmień na Release/Acquire:**
- transport.rs: `position.store()` → Release, `.load()` → Acquire (5 sites)
- transport.rs: `is_recording` store/load → Release/Acquire (5 sites)
- track_node.rs: `is_recording.store()` → Release (1 site)
- callback.rs: `record_overflow.swap()` → AcqRel (1 site)
- real_bridge.rs: `playhead_position.load()` + `is_recording.load()` → Acquire (2 sites)

**Zostaw Relaxed (18 sites) — dodaj komentarz:**
- volume, pan, mute, solo, record_armed → `// ORDERING: Relaxed OK — single-value eventual consistency`

### N1 [HIGH] — callback.rs:86 — brak catch_unwind
- Wrap body `audio_callback` w `std::panic::catch_unwind(AssertUnwindSafe(|| { ... }))`
- Na panic: wyzeruj output bufory (cisza), ustaw `AtomicBool::has_panicked`
- NIE restartuj auto — niech UI pokaże error

## Batch 2: Data Integrity + Observability (dzień 3-4)

### E3 [HIGH] — ring buffer overflow counting
8 instancji `let _ = push(...)` w callback.rs + command_processor.rs.
- Dodaj `static DROPPED_EVENTS: AtomicU32` per moduł
- Przy overflow: `DROPPED_EVENTS.fetch_add(1, Ordering::Relaxed)`
- Expose `pub fn take_dropped_event_count() -> u32` (swap z 0)

### E4 [HIGH] — topology.rs cycle detection
- `topological_sort()` → `Result<Vec<NodeId>, TopologyError::CycleDetected>`
- Caller w command_processor musi obsłużyć error

### E5 [MEDIUM] — input_capture.rs overflow counter
- Dodaj `overflow_samples: AtomicU64` do state
- Inkrementuj przy push failure, expose getter

### E6 [HIGH] — disk_io.rs write error propagation
- Count consecutive write errors
- Po 10: wyślij `EngineEvent::RecordingError { track_id, message }`
- NIE przerywaj natychmiast

### N2 [HIGH] — device_manager.rs cpal error propagation
- W cpal error callback: push `EngineEvent::DeviceError { message }`
- Dodaj wariant do EngineEvent enum w ma-core (jeśli nie istnieje)

## Batch 3: Testing (dzień 5)

### E7 [HIGH] — testy 3 modułów
- command_processor: dispatch każdego command variant
- disk_io: mock writer, test error handling
- device_manager: test device enumeration

### E8 [MEDIUM] — E2E recording overflow test
- Symuluj full ring buffer w input_capture
- Sprawdź counter + spójność nagranego pliku

## Po zakończeniu
```bash
git push -u origin fix/engine-safety
gh pr create --title "fix(engine): E1-E8,N1,N2 — safety, integrity, observability" --base main
```
