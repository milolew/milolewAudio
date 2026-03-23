//! Audio processing graph — a directed acyclic graph (DAG) of audio nodes.
//!
//! Nodes are processed in topological order (sources first, master bus last).
//! All buffers are pre-allocated at graph construction time.
//!
//! # Thread Safety
//!
//! The graph is built on a dedicated thread and swapped into the audio thread
//! via atomic pointer swap. The audio thread never allocates or deallocates
//! graph nodes or buffers.

pub mod edge;
pub mod node;
pub mod nodes;
pub mod topology;

pub use edge::Edge;
pub use node::{AudioNode, ProcessContext};
pub use topology::AudioGraph;
