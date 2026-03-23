//! # ma-audio-engine
//!
//! Real-time audio engine for milolew Audio DAW.
//!
//! This crate handles:
//! - Audio I/O via cpal (with optional ASIO support)
//! - Audio graph processing (DAG with topological sort)
//! - Transport (play, stop, pause, seek, loop)
//! - Track management with recording capabilities
//! - Lock-free communication with UI via SPSC ring buffers
//!
//! # Real-Time Safety
//!
//! The audio callback and all code called from it MUST be real-time safe:
//! - No heap allocations
//! - No mutex/rwlock
//! - No file/disk I/O
//! - No panics (use Result, return silence on error)
//! - Pre-allocated buffers only

pub mod callback;
pub mod command_processor;
pub mod disk_io;
pub mod engine;
pub mod graph;
pub mod track;
pub mod transport;
