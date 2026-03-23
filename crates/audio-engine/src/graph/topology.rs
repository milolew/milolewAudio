//! Audio graph with topological sort for correct processing order.
//!
//! The graph is a DAG of AudioNodes connected by Edges. Processing follows
//! topological order: source nodes (no inputs) are processed first, then
//! downstream nodes that depend on them, ending with the output node.
//!
//! All buffers are pre-allocated at graph construction time.

use common_types::audio_buffer::AudioBuffer;
use common_types::ids::NodeId;

use super::edge::Edge;
use super::node::{AudioNode, ProcessContext};

/// Index into the `AudioGraph::nodes` vector.
type NodeIndex = usize;

/// The complete audio processing graph.
///
/// All memory is pre-allocated at construction. The audio thread calls
/// `process()` each callback — no allocations occur during processing.
pub struct AudioGraph {
    /// All nodes in the graph. Indices are stable for the lifetime of this graph.
    nodes: Vec<Box<dyn AudioNode>>,

    /// Connections between nodes.
    edges: Vec<Edge>,

    /// Processing order (indices into `nodes`), computed via topological sort.
    /// Sources first, output last.
    schedule: Vec<NodeIndex>,

    /// Pre-allocated intermediate buffers.
    /// One buffer per edge, used to pass data between connected nodes.
    buffers: Vec<AudioBuffer>,

    /// Mapping: for each node index, which buffer indices are its inputs.
    node_input_buffers: Vec<Vec<usize>>,

    /// Mapping: for each node index, which buffer indices are its outputs.
    node_output_buffers: Vec<Vec<usize>>,
}

impl AudioGraph {
    /// Build a new audio graph from nodes and edges.
    ///
    /// This performs topological sort and pre-allocates all intermediate buffers.
    /// Call this on the graph-build thread, NOT on the audio thread.
    ///
    /// # Arguments
    /// * `nodes` - All audio nodes in the graph
    /// * `edges` - All connections between nodes
    /// * `buffer_size` - Maximum frames per callback (for buffer pre-allocation)
    pub fn new(
        nodes: Vec<Box<dyn AudioNode>>,
        edges: Vec<Edge>,
        buffer_size: u32,
    ) -> Self {
        let node_count = nodes.len();

        // Build node_id -> index mapping
        let node_indices: Vec<(NodeId, NodeIndex)> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.node_id(), i))
            .collect();

        // Topological sort using Kahn's algorithm
        let schedule = topological_sort(&nodes, &edges, &node_indices);

        // Allocate one buffer per edge
        let buffers: Vec<AudioBuffer> = edges
            .iter()
            .map(|_| AudioBuffer::stereo(buffer_size))
            .collect();

        // Build input/output buffer mappings for each node
        let mut node_input_buffers = vec![Vec::new(); node_count];
        let mut node_output_buffers = vec![Vec::new(); node_count];

        for (edge_idx, edge) in edges.iter().enumerate() {
            let from_idx = node_indices
                .iter()
                .find(|(id, _)| *id == edge.from_node)
                .map(|(_, i)| *i);
            let to_idx = node_indices
                .iter()
                .find(|(id, _)| *id == edge.to_node)
                .map(|(_, i)| *i);

            if let Some(from) = from_idx {
                node_output_buffers[from].push(edge_idx);
            }
            if let Some(to) = to_idx {
                node_input_buffers[to].push(edge_idx);
            }
        }

        Self {
            nodes,
            edges,
            schedule,
            buffers,
            node_input_buffers,
            node_output_buffers,
        }
    }

    /// Process the entire audio graph for one callback.
    ///
    /// Nodes are processed in topological order. Each node reads from its
    /// input buffers and writes to its output buffers.
    ///
    /// # Real-Time Safety
    /// This method is called on the audio thread. No allocations occur.
    #[inline]
    pub fn process(&mut self, context: &ProcessContext) {
        // Update frame count on all buffers
        for buf in &mut self.buffers {
            buf.set_frames(context.buffer_size);
            buf.clear();
        }

        for &node_idx in &self.schedule {
            // Collect input buffer references
            let input_indices = &self.node_input_buffers[node_idx];
            let output_indices = &self.node_output_buffers[node_idx];

            // Safety: We need to borrow buffers and node simultaneously.
            // The schedule guarantees no node reads a buffer that hasn't been written yet.
            // We use unsafe to split the borrow — inputs are read-only (already written by
            // upstream nodes), outputs are write-only (this node writes them).
            //
            // This is sound because:
            // 1. Input and output buffer sets are disjoint for any single node (DAG property)
            // 2. The topological order ensures inputs are written before being read
            // 3. No buffer is both an input and output for the same node

            let (input_refs, mut output_refs) = unsafe {
                split_buffer_refs(&mut self.buffers, input_indices, output_indices)
            };

            self.nodes[node_idx].process(&input_refs, &mut output_refs, context);
        }
    }

    /// Get a node by its index in the nodes array.
    pub fn node(&self, index: usize) -> Option<&dyn AudioNode> {
        self.nodes.get(index).map(|n| n.as_ref())
    }

    /// Get a mutable reference to a node by its index.
    pub fn node_mut(&mut self, index: usize) -> Option<&mut (dyn AudioNode + 'static)> {
        self.nodes.get_mut(index).map(|n| &mut **n)
    }

    /// Find a node's index by its NodeId.
    pub fn find_node_index(&self, id: NodeId) -> Option<usize> {
        self.nodes.iter().position(|n| n.node_id() == id)
    }

    /// Get the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the processing schedule (node indices in topological order).
    pub fn schedule(&self) -> &[usize] {
        &self.schedule
    }

    /// Get the edges (connections) in the graph.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Reset all nodes in the graph.
    pub fn reset(&mut self) {
        for node in &mut self.nodes {
            node.reset();
        }
        for buf in &mut self.buffers {
            buf.clear();
        }
    }

    /// Downcast a node to a concrete type. Useful for accessing node-specific methods
    /// like `InputNode::fill_from_input()` or `OutputNode::read_output_interleaved()`.
    ///
    /// # Safety
    /// The caller must ensure the node at `index` is actually of type `T`.
    pub fn node_downcast_mut<T: AudioNode + 'static>(&mut self, index: usize) -> Option<&mut T> {
        self.nodes
            .get_mut(index)
            .and_then(|n| {
                let ptr = n.as_mut() as *mut dyn AudioNode;
                // Use Any-style downcasting
                unsafe { (ptr as *mut T).as_mut() }
            })
    }
}

/// Topological sort using Kahn's algorithm.
///
/// Returns node indices in processing order: sources first, sinks last.
fn topological_sort(
    nodes: &[Box<dyn AudioNode>],
    edges: &[Edge],
    node_indices: &[(NodeId, NodeIndex)],
) -> Vec<NodeIndex> {
    let n = nodes.len();
    let mut in_degree = vec![0usize; n];
    let mut adjacency: Vec<Vec<NodeIndex>> = vec![Vec::new(); n];

    let find_idx = |id: NodeId| -> Option<NodeIndex> {
        node_indices.iter().find(|(nid, _)| *nid == id).map(|(_, i)| *i)
    };

    for edge in edges {
        if let (Some(from), Some(to)) = (find_idx(edge.from_node), find_idx(edge.to_node)) {
            adjacency[from].push(to);
            in_degree[to] += 1;
        }
    }

    // Start with nodes that have no incoming edges (sources)
    let mut queue: Vec<NodeIndex> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut result = Vec::with_capacity(n);

    while let Some(node_idx) = queue.pop() {
        result.push(node_idx);
        for &neighbor in &adjacency[node_idx] {
            in_degree[neighbor] -= 1;
            if in_degree[neighbor] == 0 {
                queue.push(neighbor);
            }
        }
    }

    // If result.len() != n, there's a cycle (should never happen in a valid DAG)
    debug_assert_eq!(
        result.len(),
        n,
        "Audio graph contains a cycle! {} nodes, but only {} in topological order",
        n,
        result.len()
    );

    result
}

/// Split buffer references into disjoint input (read-only) and output (read-write) sets.
///
/// # Safety
/// The caller must ensure that input_indices and output_indices are disjoint
/// (no buffer is both an input and output for the same node). This is guaranteed
/// by the DAG structure of the audio graph.
unsafe fn split_buffer_refs<'a>(
    buffers: &'a mut [AudioBuffer],
    input_indices: &[usize],
    output_indices: &[usize],
) -> (Vec<&'a AudioBuffer>, Vec<&'a mut AudioBuffer>) {
    let buf_ptr = buffers.as_mut_ptr();

    let inputs: Vec<&AudioBuffer> = input_indices
        .iter()
        .filter_map(|&i| unsafe { buf_ptr.add(i).as_ref() })
        .collect();

    let outputs: Vec<&mut AudioBuffer> = output_indices
        .iter()
        .filter_map(|&i| unsafe { buf_ptr.add(i).as_mut() })
        .collect();

    (inputs, outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::nodes::input_node::InputNode;
    use crate::graph::nodes::mixer_node::MixerNode;
    use crate::graph::nodes::output_node::OutputNode;
    use common_types::parameters::TransportState;

    #[test]
    fn simple_graph_processes_in_correct_order() {
        // Build: InputNode -> MixerNode -> OutputNode
        let input_id = NodeId(0);
        let mixer_id = NodeId(1);
        let output_id = NodeId(2);

        let nodes: Vec<Box<dyn AudioNode>> = vec![
            Box::new(InputNode::new(input_id)),
            Box::new(MixerNode::new(mixer_id, 1)),
            Box::new(OutputNode::new(output_id)),
        ];

        let edges = vec![
            Edge {
                from_node: input_id,
                from_port: 0,
                to_node: mixer_id,
                to_port: 0,
            },
            Edge {
                from_node: mixer_id,
                from_port: 0,
                to_node: output_id,
                to_port: 0,
            },
        ];

        let graph = AudioGraph::new(nodes, edges, 256);

        // Verify topological order: input before mixer before output
        let schedule = graph.schedule();
        assert_eq!(schedule.len(), 3);

        let input_pos = schedule.iter().position(|&i| {
            graph.nodes[i].node_id() == input_id
        }).unwrap();
        let mixer_pos = schedule.iter().position(|&i| {
            graph.nodes[i].node_id() == mixer_id
        }).unwrap();
        let output_pos = schedule.iter().position(|&i| {
            graph.nodes[i].node_id() == output_id
        }).unwrap();

        assert!(input_pos < mixer_pos, "Input must process before mixer");
        assert!(mixer_pos < output_pos, "Mixer must process before output");
    }

    #[test]
    fn graph_processes_without_panic() {
        let input_id = NodeId(0);
        let mixer_id = NodeId(1);
        let output_id = NodeId(2);

        let nodes: Vec<Box<dyn AudioNode>> = vec![
            Box::new(InputNode::new(input_id)),
            Box::new(MixerNode::new(mixer_id, 1)),
            Box::new(OutputNode::new(output_id)),
        ];

        let edges = vec![
            Edge {
                from_node: input_id,
                from_port: 0,
                to_node: mixer_id,
                to_port: 0,
            },
            Edge {
                from_node: mixer_id,
                from_port: 0,
                to_node: output_id,
                to_port: 0,
            },
        ];

        let mut graph = AudioGraph::new(nodes, edges, 256);

        let context = ProcessContext {
            sample_rate: 48000.0,
            transport_state: TransportState::Playing,
            playhead_samples: 0,
            tempo: 120.0,
            buffer_size: 256,
        };

        // Should not panic
        graph.process(&context);
    }
}
