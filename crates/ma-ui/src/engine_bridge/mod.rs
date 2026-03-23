//! Communication layer between UI and audio engine.
//!
//! Uses lock-free SPSC ring buffers (rtrb) for real-time safe communication.

pub mod bridge;
pub mod commands;
pub mod mock_engine;
pub mod real_bridge;
pub mod responses;
