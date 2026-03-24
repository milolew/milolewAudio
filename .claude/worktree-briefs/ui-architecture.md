# Brief: fix/ui-architecture

## Scope
WYŁĄCZNIE `crates/ma-ui/`. NIE DOTYKAJ `views/arrangement_view.rs`.
5 issues + 2 nowe moduły. ~5 dni.

## Krok 1: U1 [HIGH] — Split app_data.rs (dzień 1-3)

God object 765 linii → moduły:

```
ma-ui/src/
  state/
    mod.rs              // AppState struct + re-exporty
    project.rs          // ProjectState: tracks, clips, tempo, time_sig
    transport.rs        // TransportState: playing, recording, position, loop
    mixer.rs            // MixerState: volumes, pans, solos, mutes
    selection.rs        // SelectionState: selected clips/notes, current tool
    view_state.rs       // ViewState: zoom, scroll, active view, detail view
  bridge/
    mod.rs
    engine_bridge.rs    // trait EngineBridge + RealBridge (przenieś z real_bridge.rs)
    command_sender.rs   // wrap ring buffer producer + convenience methods
    event_poller.rs     // drain engine events → UI state
  sync/
    mod.rs
    engine_sync.rs      // NOWY: bidirectional state sync (poll() co frame)
    event_handler.rs    // NOWY: match EngineEvent → mutacja AppState
  app_data.rs           // SLIM ~150 linii: owns AppState + bridge, deleguje
```

Każdy moduł `state/` powinien mieć:
- Własny `#[derive(Lens)]` na struct
- Event enum + impl Model (vizia)
- Metody query (gettery) i command (mutacje + send do engine)

Po refaktorze upewnij się że WSZYSTKIE istniejące widgety kompilują się
i działają z nową strukturą. Transport bar, peak meter, fader, piano roll
— wszystko musi działać.

## Krok 2: U12 [MEDIUM] — Extract demo data (dzień 3)

115 linii inline data → `state/demo.rs`:
```rust
pub fn create_demo_project() -> ProjectState { ... }
pub fn create_demo_mixer(track_count: usize) -> MixerState { ... }
```

## Krok 3: U4 [MEDIUM] — HashMap zamiast Vec (dzień 3-4)

W `state/project.rs`:
```rust
tracks: HashMap<TrackId, TrackState>,
track_order: Vec<TrackId>,
clips: HashMap<ClipId, ClipState>,
clips_by_track: HashMap<TrackId, Vec<ClipId>>,
```

## Krok 4: sync/engine_sync.rs — state sync layer (dzień 4-5)

Bidirectional sync wywoływany co frame z vizia poll:
```rust
impl EngineSync {
    pub fn poll(&mut self, state: &mut AppState) {
        // 1. Drain EngineEvents → update state
        while let Ok(event) = self.event_consumer.pop() {
            event_handler::handle(event, state);
        }
        // 2. Read AudioFeedback → update meters + playhead
        let fb = self.feedback.read();
        state.transport.update_playhead(fb.playhead_samples);
        state.mixer.update_meters(&fb.track_meters);
    }
}
```

## Krok 5: U8 [MEDIUM] + U11 [LOW] — cleanup (dzień 5)

- U8: Dodaj overflow monitoring do command_sender
- U11: Usuń clips_for_track() jeśli unused po U4

## Po zakończeniu
```bash
git push -u origin fix/ui-architecture
gh pr create --title "refactor(ui): U1,U4,U8,U11,U12 + EngineSync layer" --base main
```
