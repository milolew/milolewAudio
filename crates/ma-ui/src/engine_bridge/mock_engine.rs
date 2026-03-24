//! Mock audio engine for standalone GUI development.
//!
//! Runs on a separate thread, reads commands, simulates transport and meters.
//! Shuts down cleanly when the shutdown flag is set.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::types::time::{Tick, PPQN};
use crate::types::track::TrackId;

use super::bridge::EngineEndpoint;
use super::commands::EngineCommand;
use super::responses::EngineResponse;

/// Simulated engine state.
struct MockState {
    is_playing: bool,
    is_recording: bool,
    position: Tick,
    tempo: f64,
}

impl Default for MockState {
    fn default() -> Self {
        Self {
            is_playing: false,
            is_recording: false,
            position: 0,
            tempo: 120.0,
        }
    }
}

impl MockState {
    /// Ticks per second at current tempo.
    fn ticks_per_second(&self) -> f64 {
        (self.tempo / 60.0) * PPQN as f64
    }

    /// Advance position by elapsed time.
    fn advance(&mut self, dt_seconds: f64) {
        if self.is_playing {
            let delta_ticks = (self.ticks_per_second() * dt_seconds) as Tick;
            self.position += delta_ticks;
        }
    }
}

/// Handle for controlling the mock engine lifecycle.
pub struct MockEngineHandle {
    shutdown: Arc<AtomicBool>,
    join_handle: Option<thread::JoinHandle<()>>,
}

impl MockEngineHandle {
    /// Signal the engine thread to stop and wait for it to finish.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MockEngineHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Spawn the mock engine on a background thread.
/// Returns a handle that shuts down the engine on drop.
pub fn spawn_mock_engine(endpoint: EngineEndpoint, track_ids: Vec<TrackId>) -> MockEngineHandle {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    let join_handle = thread::Builder::new()
        .name("mock-audio-engine".into())
        .spawn(move || run_mock_engine(endpoint, track_ids, shutdown_clone))
        .expect("Failed to spawn mock engine thread");

    MockEngineHandle {
        shutdown,
        join_handle: Some(join_handle),
    }
}

fn run_mock_engine(
    mut endpoint: EngineEndpoint,
    track_ids: Vec<TrackId>,
    shutdown: Arc<AtomicBool>,
) {
    let mut state = MockState::default();
    let mut last_tick = Instant::now();
    let frame_duration = Duration::from_millis(16); // ~60 Hz

    while !shutdown.load(Ordering::Relaxed) {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f64();
        last_tick = now;

        // Process incoming commands
        while let Ok(cmd) = endpoint.command_rx.pop() {
            match cmd {
                EngineCommand::Play => {
                    state.is_playing = true;
                    state.is_recording = false;
                }
                EngineCommand::Stop => {
                    state.is_playing = false;
                    state.is_recording = false;
                    state.position = 0;
                }
                EngineCommand::Pause => {
                    state.is_playing = false;
                }
                EngineCommand::Record => {
                    state.is_playing = true;
                    state.is_recording = true;
                }
                EngineCommand::SetPosition(tick) => {
                    state.position = tick;
                }
                EngineCommand::SetTempo(bpm) => {
                    state.tempo = bpm;
                    let _ = endpoint.response_tx.push(EngineResponse::TempoUpdate(bpm));
                }
                // Other commands are acknowledged but not simulated
                _ => {}
            }
        }

        // Advance transport
        state.advance(dt);

        // Send transport update
        let _ = endpoint.response_tx.push(EngineResponse::TransportUpdate {
            position: state.position,
            is_playing: state.is_playing,
            is_recording: state.is_recording,
        });

        // Send fake meter updates (sine wave simulation)
        let t = now.elapsed().as_secs_f64();
        for (i, track_id) in track_ids.iter().enumerate() {
            let phase = t * 2.0 + i as f64 * 0.5;
            let level = if state.is_playing {
                (0.3 + 0.2 * phase.sin() as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let _ = endpoint.response_tx.push(EngineResponse::MeterUpdate {
                track_id: *track_id,
                peak_l: level,
                peak_r: level * 0.9,
            });
        }

        // Send CPU load
        let _ = endpoint
            .response_tx
            .push(EngineResponse::CpuLoad(if state.is_playing {
                0.15
            } else {
                0.02
            }));

        // Sleep to maintain ~60Hz update rate
        let elapsed = now.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }
}
