//! Event dispatch — routes AppEvents to state mutations and engine commands.

use ma_core::device::AudioDeviceConfig;

use crate::config::{save_preferences, Preferences};
use crate::engine_bridge::commands::EngineCommand;
use crate::types::midi::Note;

use super::engine_adapter::EngineMode;
use super::{AppData, AppEvent};

pub(super) fn dispatch_transport(data: &mut AppData, event: &AppEvent) {
    match event {
        AppEvent::Play => {
            data.send_command(EngineCommand::Play);
        }
        AppEvent::Stop => {
            data.send_command(EngineCommand::Stop);
        }
        AppEvent::Record => {
            data.send_command(EngineCommand::Record);
        }
        AppEvent::Pause => {
            data.send_command(EngineCommand::Pause);
        }
        AppEvent::SetTempo(bpm) => {
            data.send_command(EngineCommand::SetTempo(*bpm));
        }
        AppEvent::SetPosition(tick) => {
            data.send_command(EngineCommand::SetPosition(*tick));
        }
        AppEvent::ToggleLoop => {
            data.transport.loop_enabled = !data.transport.loop_enabled;
        }
        _ => {}
    }
}

pub(super) fn dispatch_mixer(data: &mut AppData, event: &AppEvent) {
    match event {
        AppEvent::SetTrackVolume { track_id, volume } => {
            if let Some(track) = data.tracks.iter_mut().find(|t| t.id == *track_id) {
                track.volume = *volume;
            }
            data.send_command(EngineCommand::SetTrackVolume {
                track_id: *track_id,
                volume: *volume,
            });
        }
        AppEvent::SetTrackPan { track_id, pan } => {
            if let Some(track) = data.tracks.iter_mut().find(|t| t.id == *track_id) {
                track.pan = *pan;
            }
            data.send_command(EngineCommand::SetTrackPan {
                track_id: *track_id,
                pan: *pan,
            });
        }
        AppEvent::ToggleMute(track_id) => {
            let new_mute = if let Some(track) = data.tracks.iter_mut().find(|t| t.id == *track_id) {
                track.mute = !track.mute;
                Some(track.mute)
            } else {
                None
            };
            if let Some(mute) = new_mute {
                data.send_command(EngineCommand::SetTrackMute {
                    track_id: *track_id,
                    mute,
                });
            }
        }
        AppEvent::ToggleSolo(track_id) => {
            let new_solo = if let Some(track) = data.tracks.iter_mut().find(|t| t.id == *track_id) {
                track.solo = !track.solo;
                Some(track.solo)
            } else {
                None
            };
            if let Some(solo) = new_solo {
                data.send_command(EngineCommand::SetTrackSolo {
                    track_id: *track_id,
                    solo,
                });
            }
        }
        _ => {}
    }
}

pub(super) fn dispatch_piano_roll(data: &mut AppData, event: &AppEvent) {
    let clip_id = match data.piano_roll.active_clip_id {
        Some(id) => id,
        None => return,
    };

    match event {
        AppEvent::AddNote(note) => {
            let note = Note {
                id: data.piano_roll.alloc_note_id(),
                ..*note
            };
            if let Some(clip) = data.clips.iter().find(|c| c.id == clip_id) {
                let new_clip = clip.with_note_added(note);
                data.update_clip(new_clip);
                data.send_command(EngineCommand::AddNote { clip_id, note });
            }
        }
        AppEvent::RemoveNote(note_id) => {
            if let Some(clip) = data.clips.iter().find(|c| c.id == clip_id) {
                let new_clip = clip.with_note_removed(*note_id);
                data.update_clip(new_clip);
                data.send_command(EngineCommand::RemoveNote {
                    clip_id,
                    note_id: *note_id,
                });
            }
        }
        AppEvent::MoveNote {
            note_id,
            new_start,
            new_pitch,
        } => {
            if let Some(clip) = data.clips.iter().find(|c| c.id == clip_id) {
                if let Some(note) = clip.notes.iter().find(|n| n.id == *note_id) {
                    let updated = Note {
                        start_tick: *new_start,
                        pitch: *new_pitch,
                        ..*note
                    };
                    let new_clip = clip.with_note_updated(updated);
                    data.update_clip(new_clip);
                    data.send_command(EngineCommand::MoveNote {
                        clip_id,
                        note_id: *note_id,
                        new_start: *new_start,
                        new_pitch: *new_pitch,
                    });
                }
            }
        }
        AppEvent::ResizeNote {
            note_id,
            new_duration,
        } => {
            if let Some(clip) = data.clips.iter().find(|c| c.id == clip_id) {
                if let Some(note) = clip.notes.iter().find(|n| n.id == *note_id) {
                    let updated = Note {
                        duration_ticks: *new_duration,
                        ..*note
                    };
                    let new_clip = clip.with_note_updated(updated);
                    data.update_clip(new_clip);
                    data.send_command(EngineCommand::ResizeNote {
                        clip_id,
                        note_id: *note_id,
                        new_duration: *new_duration,
                    });
                }
            }
        }
        AppEvent::PreviewNoteOn { note, velocity } => {
            data.send_command(EngineCommand::NoteOn {
                channel: 0,
                note: *note,
                velocity: *velocity,
            });
        }
        AppEvent::PreviewNoteOff { note } => {
            data.send_command(EngineCommand::NoteOff {
                channel: 0,
                note: *note,
                velocity: 0,
            });
        }
        AppEvent::UpdateInteraction(interaction) => {
            data.piano_roll.interaction = interaction.clone();
        }
        AppEvent::SetQuantize(grid) => {
            data.piano_roll.quantize = *grid;
        }
        _ => {}
    }
}

pub(super) fn dispatch_preferences(data: &mut AppData) {
    if let EngineMode::Real { device_manager, .. } = &mut data.engine {
        device_manager.enumerate_devices();
        match device_manager.status() {
            ma_core::device::DeviceStatus::Active {
                output_device,
                actual_sample_rate,
                actual_buffer_size,
                ..
            } => {
                let latency_ms = *actual_buffer_size as f64 / *actual_sample_rate as f64 * 1000.0;
                data.device_status_text = output_device.clone();
                data.device_sample_rate = format!("{actual_sample_rate} Hz");
                data.device_buffer_size = format!("{actual_buffer_size} samples");
                data.device_latency = format!("{latency_ms:.1} ms");

                let prefs = Preferences {
                    audio: AudioDeviceConfig {
                        output_device_name: Some(output_device.clone()),
                        sample_rate: *actual_sample_rate,
                        buffer_size: *actual_buffer_size,
                        ..AudioDeviceConfig::default()
                    },
                };
                save_preferences(&prefs);
            }
            _ => {
                data.device_status_text = "Offline".into();
                data.device_sample_rate = "-".into();
                data.device_buffer_size = "-".into();
                data.device_latency = "-".into();
            }
        }
    }
}
