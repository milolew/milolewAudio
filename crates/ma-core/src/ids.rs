//! Newtype wrappers for type-safe identification of tracks, nodes, and clips.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a track in the project.
/// Uses UUID v4 for global uniqueness across sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub Uuid);

impl TrackId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TrackId {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight identifier for audio graph nodes.
/// Uses u32 instead of UUID to minimize overhead on the real-time thread.
/// Node IDs are assigned sequentially by the graph builder and are only
/// valid within a single graph instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// Unique identifier for an audio or MIDI clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClipId(pub Uuid);

impl ClipId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ClipId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_ids_are_unique() {
        let a = TrackId::new();
        let b = TrackId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn node_ids_are_comparable() {
        let a = NodeId(0);
        let b = NodeId(1);
        assert_ne!(a, b);
        assert_eq!(a, NodeId(0));
    }

    #[test]
    fn clip_ids_are_unique() {
        let a = ClipId::new();
        let b = ClipId::new();
        assert_ne!(a, b);
    }
}
