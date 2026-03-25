//! E2E smoke tests — verify complete audio pipeline from clip loading to output.
//!
//! These tests use the real audio engine (not mock) to verify that audio data
//! flows correctly through the entire graph.

use std::sync::Arc;

use ma_audio_engine::callback;
use ma_audio_engine::engine::{build_engine, EngineConfig};
use ma_audio_engine::export::{offline_render, BitDepth, ExportClip, ExportConfig};
use ma_audio_engine::graph::node::ProcessContext;
use ma_audio_engine::graph::nodes::midi_player::MidiPlayerNode;
use ma_audio_engine::graph::nodes::output_node::OutputNode;
use ma_audio_engine::graph::nodes::wav_player::{AudioClipRef, WavPlayerNode};
use ma_core::commands::EngineCommand;
use ma_core::ids::{ClipId, TrackId};
use ma_core::midi_clip::MidiClip;
use ma_core::parameters::{MidiEvent, MidiMessage, TrackConfig, TrackType, TransportState};
use ma_core::project_file::{
    load_project, save_project, ClipFile, NoteFile, ProjectFile, TrackFile, TrackKindFile,
    PROJECT_VERSION,
};
use ma_core::time::SamplePos;

/// Generate a stereo sine wave clip (non-interleaved).
fn sine_clip(freq: f32, sample_rate: u32, duration_samples: usize) -> Vec<f32> {
    let mut data = vec![0.0f32; duration_samples * 2];
    for i in 0..duration_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (t * freq * std::f32::consts::TAU).sin() * 0.5;
        data[i] = sample; // ch0
        data[duration_samples + i] = sample; // ch1
    }
    data
}

/// Compute RMS of a sample buffer.
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

// ─── Test 1: Audio playback produces non-zero output ──────────────────────

#[test]
fn playback_produces_nonzero_output() {
    let track_id = TrackId::new();
    let clip_id = ClipId::new();
    let sample_rate = 48000u32;
    let duration = 4800usize; // 0.1 second

    let config = EngineConfig {
        sample_rate,
        buffer_size: 256,
        initial_tracks: vec![(
            track_id,
            TrackConfig {
                name: "Audio 1".into(),
                channel_count: 2,
                input_enabled: false,
                initial_volume: 1.0,
                initial_pan: 0.0,
                track_type: TrackType::Audio,
            },
        )],
    };

    let (mut state, mut handle) = build_engine(config).unwrap();

    // Load a sine wave clip into the WavPlayerNode
    let clip_data = sine_clip(440.0, sample_rate, duration);
    let data: Arc<[f32]> = Arc::from(clip_data.into_boxed_slice());

    let track = state.tracks.iter().find(|t| t.id == track_id).unwrap();
    let player_idx = track.player_node_graph_index.unwrap();
    let wav_player = state
        .graph
        .node_downcast_mut::<WavPlayerNode>(player_idx)
        .unwrap();
    wav_player.add_clip(AudioClipRef {
        clip_id,
        data,
        channels: 2,
        start_sample: 0,
        length_samples: duration as SamplePos,
    });

    // Start playback via command
    handle.command_producer.push(EngineCommand::Play).unwrap();

    // Run several audio callback cycles
    let mut output = vec![0.0f32; 256 * 2];
    let mut total_rms = 0.0f32;
    for _ in 0..10 {
        callback::audio_callback(&mut state, &mut output, 256);
        total_rms += rms(&output);
    }

    assert!(
        total_rms > 0.1,
        "Expected non-zero cumulative RMS across 10 callbacks, got {total_rms}"
    );
}

// ─── Test 2: MIDI playback produces audio ─────────────────────────────────

#[test]
fn midi_playback_produces_audio() {
    let track_id = TrackId::new();
    let clip_id = ClipId::new();
    let sample_rate = 48000u32;

    let config = EngineConfig {
        sample_rate,
        buffer_size: 256,
        initial_tracks: vec![(
            track_id,
            TrackConfig {
                name: "MIDI 1".into(),
                channel_count: 2,
                input_enabled: false,
                initial_volume: 1.0,
                initial_pan: 0.0,
                track_type: TrackType::Midi,
            },
        )],
    };

    let (mut state, mut handle) = build_engine(config).unwrap();

    // Create a MIDI clip with a note
    let events = vec![
        MidiEvent {
            tick: 0,
            message: MidiMessage::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            },
        },
        MidiEvent {
            tick: 960,
            message: MidiMessage::NoteOff {
                channel: 0,
                note: 60,
                velocity: 0,
            },
        },
    ];
    let midi_clip = Arc::new(MidiClip::new(events, 960));

    // Install MIDI clip via command
    handle
        .command_producer
        .push(EngineCommand::InstallMidiClip {
            track_id,
            clip_id,
            clip: midi_clip,
            start_tick: 0,
        })
        .unwrap();
    handle.command_producer.push(EngineCommand::Play).unwrap();

    // Run callbacks
    let mut output = vec![0.0f32; 256 * 2];
    let mut total_rms = 0.0f32;
    for _ in 0..20 {
        callback::audio_callback(&mut state, &mut output, 256);
        total_rms += rms(&output);
    }

    assert!(
        total_rms > 0.01,
        "Expected MIDI synth to produce audio, cumulative RMS: {total_rms}"
    );
}

// ─── Test 3: Export produces valid WAV ────────────────────────────────────

#[test]
fn export_produces_valid_wav() {
    let track_id = TrackId::new();
    let clip_id = ClipId::new();
    let sample_rate = 48000u32;
    let duration = 24000usize; // 0.5 second

    let config = EngineConfig {
        sample_rate,
        buffer_size: 256,
        initial_tracks: vec![(
            track_id,
            TrackConfig {
                name: "Audio 1".into(),
                channel_count: 2,
                input_enabled: false,
                initial_volume: 1.0,
                initial_pan: 0.0,
                track_type: TrackType::Audio,
            },
        )],
    };

    let clip_data = sine_clip(440.0, sample_rate, duration);
    let clips = vec![ExportClip {
        track_id,
        clip_id,
        data: Arc::from(clip_data.into_boxed_slice()),
        channels: 2,
        start_sample: 0,
        length_samples: duration as SamplePos,
    }];

    let output_path = std::env::temp_dir().join("e2e_export_test.wav");
    let export_config = ExportConfig {
        sample_rate,
        bit_depth: BitDepth::ThirtyTwoFloat,
    };

    offline_render(
        config,
        &clips,
        &[],
        duration as u64,
        &output_path,
        &export_config,
    )
    .unwrap();

    // Verify WAV
    let reader = hound::WavReader::open(&output_path).unwrap();
    assert_eq!(reader.spec().sample_rate, sample_rate);
    assert_eq!(reader.spec().channels, 2);

    let samples: Vec<f32> = reader.into_samples::<f32>().map(|s| s.unwrap()).collect();

    // Should have approximately duration * channels samples
    let expected_min = duration; // at least half
    assert!(
        samples.len() >= expected_min,
        "Expected at least {expected_min} samples, got {}",
        samples.len()
    );

    // Non-zero audio
    let wav_rms = rms(&samples);
    assert!(
        wav_rms > 0.01,
        "Expected non-zero RMS in WAV, got {wav_rms}"
    );

    std::fs::remove_file(&output_path).ok();
}

// ─── Test 4: Project save/load round-trip ─────────────────────────────────

#[test]
fn project_save_load_roundtrip() {
    let track1_id = TrackId::new();
    let track2_id = TrackId::new();
    let clip1_id = ClipId::new();
    let clip2_id = ClipId::new();

    let project = ProjectFile {
        version: PROJECT_VERSION,
        name: "E2E Test Project".into(),
        tempo: 140.0,
        sample_rate: 48000,
        tracks: vec![
            TrackFile {
                id: track1_id,
                name: "Melody".into(),
                kind: TrackKindFile::Midi,
                color: [100, 160, 255],
                volume: 0.8,
                pan: 0.0,
                muted: false,
                clips: vec![ClipFile {
                    id: clip1_id,
                    name: "Melody A".into(),
                    start_tick: 0,
                    duration_ticks: 7680,
                    notes: vec![
                        NoteFile {
                            pitch: 60,
                            start_tick: 0,
                            duration_ticks: 480,
                            velocity: 100,
                            channel: 0,
                        },
                        NoteFile {
                            pitch: 64,
                            start_tick: 480,
                            duration_ticks: 480,
                            velocity: 90,
                            channel: 0,
                        },
                        NoteFile {
                            pitch: 67,
                            start_tick: 960,
                            duration_ticks: 960,
                            velocity: 110,
                            channel: 0,
                        },
                    ],
                    audio_file: None,
                    audio_length_samples: None,
                    audio_sample_rate: None,
                }],
            },
            TrackFile {
                id: track2_id,
                name: "Drums".into(),
                kind: TrackKindFile::Audio,
                color: [80, 220, 120],
                volume: 1.0,
                pan: -0.2,
                muted: false,
                clips: vec![ClipFile {
                    id: clip2_id,
                    name: "Drum Loop".into(),
                    start_tick: 0,
                    duration_ticks: 15360,
                    notes: vec![],
                    audio_file: Some("audio/drums.wav".into()),
                    audio_length_samples: Some(48000),
                    audio_sample_rate: Some(48000),
                }],
            },
        ],
    };

    let path = std::env::temp_dir().join("e2e_project_roundtrip.json");

    // Save
    save_project(&project, &path).unwrap();

    // Load
    let loaded = load_project(&path).unwrap();

    // Verify all fields
    assert_eq!(loaded.version, PROJECT_VERSION);
    assert_eq!(loaded.name, "E2E Test Project");
    assert!((loaded.tempo - 140.0).abs() < f64::EPSILON);
    assert_eq!(loaded.sample_rate, 48000);
    assert_eq!(loaded.tracks.len(), 2);

    // Track 1 (MIDI)
    let t1 = &loaded.tracks[0];
    assert_eq!(t1.id, track1_id);
    assert_eq!(t1.name, "Melody");
    assert_eq!(t1.kind, TrackKindFile::Midi);
    assert_eq!(t1.color, [100, 160, 255]);
    assert!((t1.volume - 0.8).abs() < f32::EPSILON);
    assert_eq!(t1.clips.len(), 1);
    assert_eq!(t1.clips[0].notes.len(), 3);
    assert_eq!(
        t1.clips[0].notes[0],
        NoteFile {
            pitch: 60,
            start_tick: 0,
            duration_ticks: 480,
            velocity: 100,
            channel: 0,
        }
    );

    // Track 2 (Audio)
    let t2 = &loaded.tracks[1];
    assert_eq!(t2.id, track2_id);
    assert_eq!(t2.kind, TrackKindFile::Audio);
    assert!((t2.pan - (-0.2)).abs() < f32::EPSILON);
    assert_eq!(t2.clips[0].audio_file, Some("audio/drums.wav".into()));
    assert_eq!(t2.clips[0].audio_length_samples, Some(48000));

    std::fs::remove_file(&path).ok();
}
