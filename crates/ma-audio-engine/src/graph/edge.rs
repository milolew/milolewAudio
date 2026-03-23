//! Connections between audio graph nodes.

use ma_core::ids::NodeId;

/// A directed edge in the audio graph: source output port → destination input port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// Source node.
    pub from_node: NodeId,
    /// Source node's output port index.
    pub from_port: usize,
    /// Destination node.
    pub to_node: NodeId,
    /// Destination node's input port index.
    pub to_port: usize,
}
