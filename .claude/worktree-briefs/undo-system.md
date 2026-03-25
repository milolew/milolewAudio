# Brief: Undo/Redo System

## Scope
- `ma-core/src/undo.rs` (nowy plik)
- `ma-ui/src/app_data/` (integracja)
- ~4 dni

## NIE DOTYKAJ
views/, widgets/, ma-audio-engine

## Architektura — Command Pattern z UndoStack

```rust
// ma-core/src/undo.rs
pub trait UndoAction: Send + 'static {
    fn description(&self) -> &str;
    fn apply(&self, state: &mut dyn std::any::Any);
    fn revert(&self, state: &mut dyn std::any::Any);
}

pub struct UndoManager {
    stack: Vec<Box<dyn UndoAction>>,
    cursor: usize,  // wskaźnik aktualnej pozycji
    max_depth: usize, // domyślnie 100
}
```

Push truncuje redo stack (drop everything after cursor).

## UndoAction implementacje (na start)
- MoveClipAction
- AddClipAction
- RemoveClipAction
- SplitClipAction
- DuplicateClipAction
- AddNoteAction
- RemoveNoteAction
- MoveNoteAction
- ResizeNoteAction
- SetTrackVolumeAction
- AddTrackAction
- RemoveTrackAction
- RenameTrackAction
- TransposeNotesAction
- QuantizeNotesAction

## Integracja z UI
- AppEvent::Undo → undo_manager.undo()
- AppEvent::Redo → undo_manager.redo()

## Testy
- push/undo/redo cykl
- max_depth truncation
- empty stack edge cases

## Po zakończeniu
```bash
git push && gh pr create --base main
```
