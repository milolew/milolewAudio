//! Offline export/mixdown — render the audio graph to a WAV file.
//!
//! Builds a standalone audio engine, processes the graph in a tight loop
//! (faster than real-time), and writes the output to disk via hound.

use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::callback::CallbackState;
use crate::engine::{build_engine, EngineConfig};
use crate::graph::node::ProcessContext;
use crate::graph::nodes::output_node::OutputNode;
use crate::graph::nodes::wav_player::AudioClipRef;
use ma_core::ids::{ClipId, TrackId};
use ma_core::parameters::TransportState;
use ma_core::time::SamplePos;

/// Bit depth for exported audio.
#[derive(Debug, Clone, Copy)]
pub enum BitDepth {
    /// 16-bit integer with TPDF dithering.
    Sixteen,
    /// 32-bit IEEE float.
    ThirtyTwoFloat,
}

/// Configuration for offline export.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub sample_rate: u32,
    pub bit_depth: BitDepth,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            bit_depth: BitDepth::ThirtyTwoFloat,
        }
    }
}

/// Errors that can occur during export.
#[derive(Debug)]
pub enum ExportError {
    /// Audio graph could not be built.
    GraphBuild(String),
    /// WAV file could not be written.
    Io(std::io::Error),
    /// Hound writer error.
    Wav(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GraphBuild(msg) => write!(f, "graph build error: {msg}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Wav(msg) => write!(f, "WAV error: {msg}"),
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// A clip to be loaded into the export engine.
pub struct ExportClip {
    pub track_id: TrackId,
    pub clip_id: ClipId,
    pub data: std::sync::Arc<[f32]>,
    pub channels: usize,
    pub start_sample: SamplePos,
    pub length_samples: SamplePos,
}

/// Render the audio graph offline and write to a WAV file.
///
/// Builds a standalone engine from the given config, loads clips into the graph,
/// then processes in a tight loop writing output to disk.
///
/// # Arguments
/// * `engine_config` — engine setup (sample rate, buffer size, tracks)
/// * `clips` — audio clips to load before rendering
/// * `total_samples` — total duration to render in samples
/// * `output_path` — destination WAV file
/// * `export_config` — bit depth and sample rate
pub fn offline_render(
    engine_config: EngineConfig,
    clips: &[ExportClip],
    total_samples: u64,
    output_path: &Path,
    export_config: &ExportConfig,
) -> Result<(), ExportError> {
    let (mut state, _handle) =
        build_engine(engine_config).map_err(|e| ExportError::GraphBuild(e.to_string()))?;

    // Load clips into the appropriate WavPlayerNodes
    for clip in clips {
        load_clip_into_graph(&mut state, clip);
    }

    // Start transport
    state.transport.play();

    let buffer_size = 256u32;
    let channels = 2u16;

    let spec = match export_config.bit_depth {
        BitDepth::Sixteen => WavSpec {
            channels,
            sample_rate: export_config.sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        },
        BitDepth::ThirtyTwoFloat => WavSpec {
            channels,
            sample_rate: export_config.sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        },
    };

    let mut writer =
        WavWriter::create(output_path, spec).map_err(|e| ExportError::Wav(e.to_string()))?;
    let mut interleaved = vec![0.0f32; buffer_size as usize * channels as usize];
    let mut dither_state = DitherState::new();

    while state.transport.position() < total_samples as SamplePos {
        let playhead = state.transport.advance(buffer_size);

        let any_solo = state
            .tracks
            .iter()
            .any(|t| t.solo.load(std::sync::atomic::Ordering::Relaxed));

        let context = ProcessContext {
            sample_rate: export_config.sample_rate as f32,
            transport_state: TransportState::Playing,
            playhead_samples: playhead,
            tempo: state.transport.tempo(),
            buffer_size,
            any_solo,
        };

        state.graph.process(&context);

        // Read output from OutputNode
        if let Some(output_idx) = state.output_node_index {
            if let Some(output_node) = state.graph.node_downcast_mut::<OutputNode>(output_idx) {
                output_node.read_output_interleaved(&mut interleaved);
            }
        }

        // Determine how many frames to write (don't exceed total)
        let remaining = total_samples as i64 - playhead;
        let frames_to_write = (buffer_size as i64).min(remaining.max(0)) as usize;
        let samples_to_write = frames_to_write * channels as usize;

        match export_config.bit_depth {
            BitDepth::ThirtyTwoFloat => {
                for &sample in &interleaved[..samples_to_write] {
                    writer
                        .write_sample(sample)
                        .map_err(|e| ExportError::Wav(e.to_string()))?;
                }
            }
            BitDepth::Sixteen => {
                for &sample in &interleaved[..samples_to_write] {
                    let dithered = dither_state.tpdf_dither(sample);
                    writer
                        .write_sample(dithered)
                        .map_err(|e| ExportError::Wav(e.to_string()))?;
                }
            }
        }
    }

    writer
        .finalize()
        .map_err(|e| ExportError::Wav(e.to_string()))?;

    Ok(())
}

/// Load a clip into the appropriate WavPlayerNode in the graph.
fn load_clip_into_graph(state: &mut CallbackState, clip: &ExportClip) {
    let track = state.tracks.iter().find(|t| t.id == clip.track_id);
    let player_idx = match track {
        Some(t) => t.player_node_graph_index,
        None => return,
    };

    if let Some(idx) = player_idx {
        if let Some(wav_player) = state
            .graph
            .node_downcast_mut::<crate::graph::nodes::wav_player::WavPlayerNode>(idx)
        {
            wav_player.add_clip(AudioClipRef {
                clip_id: clip.clip_id,
                data: std::sync::Arc::clone(&clip.data),
                channels: clip.channels,
                start_sample: clip.start_sample,
                length_samples: clip.length_samples,
            });
        }
    }
}

/// Simple xorshift PRNG for TPDF dithering — avoids pulling in `rand` crate.
struct DitherState {
    state: u32,
}

impl DitherState {
    fn new() -> Self {
        Self { state: 0x12345678 }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// TPDF dither: add triangular-PDF noise before quantizing to 16-bit.
    fn tpdf_dither(&mut self, sample: f32) -> i16 {
        let r1 = (self.next_u32() as f32 / u32::MAX as f32) - 0.5;
        let r2 = (self.next_u32() as f32 / u32::MAX as f32) - 0.5;
        let dither = (r1 + r2) / 32768.0; // +-1 LSB triangular
        let scaled = sample * 32767.0 + dither;
        scaled.round().clamp(-32768.0, 32767.0) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineConfig;
    use ma_core::ids::{ClipId, TrackId};
    use ma_core::parameters::{TrackConfig, TrackType};

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

    #[test]
    fn export_float32_produces_valid_wav() {
        let track_id = TrackId::new();
        let clip_id = ClipId::new();
        let sample_rate = 48000;
        let duration = 48000; // 1 second

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
            data: std::sync::Arc::from(clip_data.into_boxed_slice()),
            channels: 2,
            start_sample: 0,
            length_samples: duration as SamplePos,
        }];

        let output_path = std::env::temp_dir().join("test_export_f32.wav");
        let export_config = ExportConfig {
            sample_rate,
            bit_depth: BitDepth::ThirtyTwoFloat,
        };

        offline_render(
            config,
            &clips,
            duration as u64,
            &output_path,
            &export_config,
        )
        .unwrap();

        // Verify WAV file
        let reader = hound::WavReader::open(&output_path).unwrap();
        assert_eq!(reader.spec().sample_rate, sample_rate);
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().bits_per_sample, 32);

        let samples: Vec<f32> = reader.into_samples::<f32>().map(|s| s.unwrap()).collect();
        assert!(!samples.is_empty());

        // Verify non-zero RMS
        let rms: f32 = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        assert!(rms > 0.01, "Expected non-zero RMS, got {rms}");

        std::fs::remove_file(&output_path).ok();
    }

    #[test]
    fn export_16bit_produces_valid_wav() {
        let track_id = TrackId::new();
        let clip_id = ClipId::new();
        let sample_rate = 44100;
        let duration = 4410; // 0.1 second

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
            data: std::sync::Arc::from(clip_data.into_boxed_slice()),
            channels: 2,
            start_sample: 0,
            length_samples: duration as SamplePos,
        }];

        let output_path = std::env::temp_dir().join("test_export_16bit.wav");
        let export_config = ExportConfig {
            sample_rate,
            bit_depth: BitDepth::Sixteen,
        };

        offline_render(
            config,
            &clips,
            duration as u64,
            &output_path,
            &export_config,
        )
        .unwrap();

        let reader = hound::WavReader::open(&output_path).unwrap();
        assert_eq!(reader.spec().bits_per_sample, 16);
        assert_eq!(reader.spec().sample_format, SampleFormat::Int);

        let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
        let has_nonzero = samples.iter().any(|&s| s != 0);
        assert!(has_nonzero, "Expected non-zero samples in 16-bit export");

        std::fs::remove_file(&output_path).ok();
    }
}
