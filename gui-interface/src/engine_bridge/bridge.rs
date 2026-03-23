//! EngineBridge — lock-free communication between UI and audio engine.
//!
//! Uses rtrb SPSC ring buffers. The UI side sends commands and polls responses.

use rtrb::{Consumer, Producer, RingBuffer};

use super::commands::EngineCommand;
use super::responses::EngineResponse;

const DEFAULT_COMMAND_CAPACITY: usize = 256;
const DEFAULT_RESPONSE_CAPACITY: usize = 1024;

/// UI-side handle for communicating with the audio engine.
pub struct EngineBridge {
    command_tx: Producer<EngineCommand>,
    response_rx: Consumer<EngineResponse>,
}

/// Engine-side handle — given to the audio engine (or mock).
pub struct EngineEndpoint {
    pub command_rx: Consumer<EngineCommand>,
    pub response_tx: Producer<EngineResponse>,
}

/// Create a matched pair of (UI-side bridge, engine-side endpoint).
pub fn create_bridge() -> (EngineBridge, EngineEndpoint) {
    create_bridge_with_capacity(DEFAULT_COMMAND_CAPACITY, DEFAULT_RESPONSE_CAPACITY)
}

pub fn create_bridge_with_capacity(
    cmd_capacity: usize,
    resp_capacity: usize,
) -> (EngineBridge, EngineEndpoint) {
    let (cmd_tx, cmd_rx) = RingBuffer::new(cmd_capacity);
    let (resp_tx, resp_rx) = RingBuffer::new(resp_capacity);

    let bridge = EngineBridge {
        command_tx: cmd_tx,
        response_rx: resp_rx,
    };

    let endpoint = EngineEndpoint {
        command_rx: cmd_rx,
        response_tx: resp_tx,
    };

    (bridge, endpoint)
}

impl EngineBridge {
    /// Send a command to the engine. Returns false if the ring buffer is full.
    pub fn send_command(&mut self, cmd: EngineCommand) -> bool {
        match self.command_tx.push(cmd) {
            Ok(()) => true,
            Err(_) => {
                log::warn!("Engine command ring buffer full, dropping command");
                false
            }
        }
    }

    /// Drain all available responses from the engine. Called once per frame.
    pub fn poll_responses(&mut self) -> Vec<EngineResponse> {
        let mut responses = Vec::new();
        while let Ok(resp) = self.response_rx.pop() {
            responses.push(resp);
        }
        responses
    }
}
