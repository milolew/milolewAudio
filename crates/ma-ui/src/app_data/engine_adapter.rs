//! Engine adapter — abstracts over real and mock audio engine connections.
//!
//! Handles command translation (UI → core), response polling, and engine lifecycle.

use ma_audio_engine::device_manager::AudioDeviceManager;
use ma_audio_engine::engine::EngineConfig;
use ma_core::commands::EngineCommand as CoreCommand;
use ma_core::device::AudioDeviceConfig;

use crate::engine_bridge::bridge::{create_bridge, EngineBridge};
use crate::engine_bridge::commands::EngineCommand;
use crate::engine_bridge::mock_engine::{spawn_mock_engine, MockEngineHandle};
use crate::engine_bridge::real_bridge::RealEngineBridge;
use crate::engine_bridge::responses::EngineResponse;
use crate::state::mixer_state::MixerState;
use crate::state::transport_state::TransportState;
use crate::types::track::TrackId;

/// Engine connection mode.
pub enum EngineMode {
    Real {
        device_manager: Box<AudioDeviceManager>,
        bridge: RealEngineBridge,
    },
    Mock {
        bridge: EngineBridge,
        _handle: MockEngineHandle,
    },
}

/// Attempt to start real audio engine with the given device config.
pub(super) fn try_real_engine(device_config: &AudioDeviceConfig) -> Result<EngineMode, String> {
    let mut device_manager = AudioDeviceManager::new();
    device_manager.enumerate_devices();
    let engine_config = EngineConfig::default();
    let handle = device_manager
        .apply_config(device_config.clone(), engine_config)
        .map_err(|e| e.to_string())?;
    let bridge = RealEngineBridge::new(handle);
    Ok(EngineMode::Real {
        device_manager: Box::new(device_manager),
        bridge,
    })
}

/// Create a mock engine fallback.
pub(super) fn create_mock_engine(track_ids: Vec<TrackId>) -> EngineMode {
    let (bridge, endpoint) = create_bridge();
    let handle = spawn_mock_engine(endpoint, track_ids);
    EngineMode::Mock {
        bridge,
        _handle: handle,
    }
}

/// Send a UI command to whichever engine is active.
pub(super) fn send_command(engine: &mut EngineMode, cmd: EngineCommand) {
    let sent = match engine {
        EngineMode::Real { bridge, .. } => translate_command(&cmd)
            .map(|core_cmd| bridge.send_command(core_cmd))
            .unwrap_or(true),
        EngineMode::Mock { bridge, .. } => bridge.send_command(cmd),
    };
    if !sent {
        log::error!("Engine command dropped — ring buffer full");
    }
}

/// Translate UI command to core engine command.
fn translate_command(cmd: &EngineCommand) -> Option<CoreCommand> {
    match cmd {
        EngineCommand::Play => Some(CoreCommand::Play),
        EngineCommand::Stop => Some(CoreCommand::Stop),
        EngineCommand::Pause => Some(CoreCommand::Pause),
        EngineCommand::Record => Some(CoreCommand::StartRecording),
        EngineCommand::SetTempo(bpm) => Some(CoreCommand::SetTempo(*bpm)),
        EngineCommand::SetTrackVolume { track_id, volume } => Some(CoreCommand::SetTrackVolume {
            track_id: *track_id,
            volume: *volume,
        }),
        EngineCommand::SetTrackPan { track_id, pan } => Some(CoreCommand::SetTrackPan {
            track_id: *track_id,
            pan: *pan,
        }),
        EngineCommand::SetTrackMute { track_id, mute } => Some(CoreCommand::SetTrackMute {
            track_id: *track_id,
            mute: *mute,
        }),
        EngineCommand::SetTrackSolo { track_id, solo } => Some(CoreCommand::SetTrackSolo {
            track_id: *track_id,
            solo: *solo,
        }),
        _ => None,
    }
}

/// Poll engine responses and update transport/mixer state.
///
/// Takes individual `&mut` references to avoid double-borrow of AppData.
pub(super) fn poll_engine(
    engine: &mut EngineMode,
    response_buf: &mut Vec<EngineResponse>,
    transport: &mut TransportState,
    mixer: &mut MixerState,
) {
    let mut responses = std::mem::take(response_buf);
    match engine {
        EngineMode::Real { bridge, .. } => bridge.poll_responses(&mut responses),
        EngineMode::Mock { bridge, .. } => bridge.poll_responses(&mut responses),
    };
    for resp in &responses {
        match resp {
            EngineResponse::TransportUpdate {
                position,
                is_playing,
                is_recording,
            } => {
                transport.position = *position;
                transport.is_playing = *is_playing;
                transport.is_recording = *is_recording;
            }
            EngineResponse::TempoUpdate(bpm) => {
                transport.tempo = *bpm;
            }
            EngineResponse::MeterUpdate {
                track_id,
                peak_l,
                peak_r,
            } => {
                mixer.update_meter(*track_id, *peak_l, *peak_r);
            }
            EngineResponse::CpuLoad(load) => {
                mixer.cpu_load = *load;
            }
        }
    }
    // Return the buffer for reuse next frame
    *response_buf = responses;
}
