//! Undo/Redo system — generic Command Pattern implementation.
//!
//! Provides [`UndoAction`] trait and [`UndoManager`] with stack + cursor semantics.
//! Generic over state type `S` so ma-core has no dependency on ma-ui.

/// A reversible action that can be applied to and reverted from state `S`.
///
/// Implementations must capture enough data to perform both the forward
/// operation and its inverse. Actions are immutable command objects —
/// old and new values are captured at creation time.
pub trait UndoAction<S>: Send + 'static {
    /// Human-readable description (e.g. "Move Note", "Add Clip").
    fn description(&self) -> &str;

    /// Apply the action (forward). Called on redo.
    fn apply(&self, state: &mut S);

    /// Revert the action (backward). Called on undo.
    fn revert(&self, state: &mut S);
}

/// Manages a linear undo/redo stack with cursor semantics.
///
/// Actions at indices `[0..cursor)` have been applied.
/// Actions at `[cursor..len)` are available for redo.
/// Pushing a new action truncates the redo tail.
pub struct UndoManager<S> {
    stack: Vec<Box<dyn UndoAction<S>>>,
    cursor: usize,
    max_depth: usize,
}

impl<S: 'static> UndoManager<S> {
    /// Create a new UndoManager with the given maximum history depth.
    pub fn new(max_depth: usize) -> Self {
        Self {
            stack: Vec::new(),
            cursor: 0,
            max_depth,
        }
    }

    /// Push a new action onto the stack.
    ///
    /// The action is NOT applied here — the caller must apply it before pushing.
    /// Truncates any redo tail (everything after cursor).
    /// If the stack exceeds `max_depth`, the oldest action is dropped.
    pub fn push(&mut self, action: Box<dyn UndoAction<S>>) {
        self.stack.truncate(self.cursor);
        self.stack.push(action);
        self.cursor += 1;

        if self.stack.len() > self.max_depth {
            self.stack.remove(0);
            self.cursor -= 1;
        }
    }

    /// Undo the most recent action. Returns the description of the undone action.
    pub fn undo(&mut self, state: &mut S) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.stack[self.cursor].revert(state);
        Some(self.stack[self.cursor].description())
    }

    /// Redo the next action. Returns the description of the redone action.
    pub fn redo(&mut self, state: &mut S) -> Option<&str> {
        if self.cursor >= self.stack.len() {
            return None;
        }
        self.stack[self.cursor].apply(state);
        self.cursor += 1;
        Some(self.stack[self.cursor - 1].description())
    }

    /// Whether there is an action available to undo.
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// Whether there is an action available to redo.
    pub fn can_redo(&self) -> bool {
        self.cursor < self.stack.len()
    }

    /// Description of the action that would be undone, if any.
    pub fn undo_description(&self) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        Some(self.stack[self.cursor - 1].description())
    }

    /// Description of the action that would be redone, if any.
    pub fn redo_description(&self) -> Option<&str> {
        if self.cursor >= self.stack.len() {
            return None;
        }
        Some(self.stack[self.cursor].description())
    }

    /// Clear all undo/redo history.
    pub fn clear(&mut self) {
        self.stack.clear();
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter {
        value: i32,
    }

    struct IncrementAction {
        amount: i32,
    }

    impl UndoAction<Counter> for IncrementAction {
        fn description(&self) -> &str {
            "Increment"
        }

        fn apply(&self, state: &mut Counter) {
            state.value += self.amount;
        }

        fn revert(&self, state: &mut Counter) {
            state.value -= self.amount;
        }
    }

    struct SetValueAction {
        old_value: i32,
        new_value: i32,
    }

    impl UndoAction<Counter> for SetValueAction {
        fn description(&self) -> &str {
            "Set Value"
        }

        fn apply(&self, state: &mut Counter) {
            state.value = self.new_value;
        }

        fn revert(&self, state: &mut Counter) {
            state.value = self.old_value;
        }
    }

    #[test]
    fn push_and_undo() {
        let mut mgr = UndoManager::new(100);
        let mut state = Counter { value: 0 };

        state.value += 5;
        mgr.push(Box::new(IncrementAction { amount: 5 }));

        assert_eq!(state.value, 5);
        mgr.undo(&mut state);
        assert_eq!(state.value, 0);
    }

    #[test]
    fn push_and_redo() {
        let mut mgr = UndoManager::new(100);
        let mut state = Counter { value: 0 };

        state.value += 5;
        mgr.push(Box::new(IncrementAction { amount: 5 }));

        mgr.undo(&mut state);
        assert_eq!(state.value, 0);

        mgr.redo(&mut state);
        assert_eq!(state.value, 5);
    }

    #[test]
    fn multiple_undo_redo() {
        let mut mgr = UndoManager::new(100);
        let mut state = Counter { value: 0 };

        for i in 1..=3 {
            state.value += i;
            mgr.push(Box::new(IncrementAction { amount: i }));
        }
        assert_eq!(state.value, 6); // 1+2+3

        // Undo all
        mgr.undo(&mut state);
        assert_eq!(state.value, 3); // 6-3
        mgr.undo(&mut state);
        assert_eq!(state.value, 1); // 3-2
        mgr.undo(&mut state);
        assert_eq!(state.value, 0); // 1-1

        // Redo all
        mgr.redo(&mut state);
        assert_eq!(state.value, 1);
        mgr.redo(&mut state);
        assert_eq!(state.value, 3);
        mgr.redo(&mut state);
        assert_eq!(state.value, 6);
    }

    #[test]
    fn redo_truncation_on_push() {
        let mut mgr = UndoManager::new(100);
        let mut state = Counter { value: 0 };

        // Push A(+1), B(+2)
        state.value += 1;
        mgr.push(Box::new(IncrementAction { amount: 1 }));
        state.value += 2;
        mgr.push(Box::new(IncrementAction { amount: 2 }));
        assert_eq!(state.value, 3);

        // Undo B → state=1, B is in redo
        mgr.undo(&mut state);
        assert_eq!(state.value, 1);
        assert!(mgr.can_redo());

        // Push C(+10) → B is gone from redo
        state.value += 10;
        mgr.push(Box::new(IncrementAction { amount: 10 }));
        assert_eq!(state.value, 11);
        assert!(!mgr.can_redo());

        // Undo C → state=1
        mgr.undo(&mut state);
        assert_eq!(state.value, 1);

        // Undo A → state=0
        mgr.undo(&mut state);
        assert_eq!(state.value, 0);

        // No more undo
        assert!(!mgr.can_undo());
    }

    #[test]
    fn max_depth_drops_oldest() {
        let mut mgr = UndoManager::new(3);
        let mut state = Counter { value: 0 };

        for i in 1..=5 {
            state.value = i * 10;
            mgr.push(Box::new(SetValueAction {
                old_value: (i - 1) * 10,
                new_value: i * 10,
            }));
        }
        assert_eq!(state.value, 50);

        // Only 3 undos available (actions 3→30, 4→40, 5→50)
        let desc = mgr.undo(&mut state);
        assert_eq!(desc, Some("Set Value"));
        assert_eq!(state.value, 40);

        mgr.undo(&mut state);
        assert_eq!(state.value, 30);

        mgr.undo(&mut state);
        assert_eq!(state.value, 20);

        // No more undo — oldest were dropped
        assert!(!mgr.can_undo());
        assert_eq!(mgr.undo(&mut state), None);
    }

    #[test]
    fn empty_undo_returns_none() {
        let mut mgr: UndoManager<Counter> = UndoManager::new(100);
        let mut state = Counter { value: 42 };

        assert_eq!(mgr.undo(&mut state), None);
        assert_eq!(state.value, 42);
    }

    #[test]
    fn empty_redo_returns_none() {
        let mut mgr: UndoManager<Counter> = UndoManager::new(100);
        let mut state = Counter { value: 42 };

        assert_eq!(mgr.redo(&mut state), None);
        assert_eq!(state.value, 42);

        // Also after push without undo
        state.value += 1;
        mgr.push(Box::new(IncrementAction { amount: 1 }));
        assert_eq!(mgr.redo(&mut state), None);
    }

    #[test]
    fn can_undo_can_redo_flags() {
        let mut mgr = UndoManager::new(100);
        let mut state = Counter { value: 0 };

        assert!(!mgr.can_undo());
        assert!(!mgr.can_redo());

        state.value += 1;
        mgr.push(Box::new(IncrementAction { amount: 1 }));
        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());

        mgr.undo(&mut state);
        assert!(!mgr.can_undo());
        assert!(mgr.can_redo());

        mgr.redo(&mut state);
        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());
    }

    #[test]
    fn descriptions() {
        let mut mgr = UndoManager::new(100);
        let mut state = Counter { value: 0 };

        assert_eq!(mgr.undo_description(), None);
        assert_eq!(mgr.redo_description(), None);

        state.value += 1;
        mgr.push(Box::new(IncrementAction { amount: 1 }));
        assert_eq!(mgr.undo_description(), Some("Increment"));
        assert_eq!(mgr.redo_description(), None);

        mgr.undo(&mut state);
        assert_eq!(mgr.undo_description(), None);
        assert_eq!(mgr.redo_description(), Some("Increment"));
    }

    #[test]
    fn clear_resets_everything() {
        let mut mgr = UndoManager::new(100);
        let mut state = Counter { value: 0 };

        state.value += 1;
        mgr.push(Box::new(IncrementAction { amount: 1 }));
        state.value += 2;
        mgr.push(Box::new(IncrementAction { amount: 2 }));

        mgr.clear();

        assert!(!mgr.can_undo());
        assert!(!mgr.can_redo());
        assert_eq!(mgr.undo(&mut state), None);
        assert_eq!(mgr.redo(&mut state), None);
    }
}
