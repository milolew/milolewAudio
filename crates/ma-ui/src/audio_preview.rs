//! Audio preview — plays audio files via a separate cpal output stream.
//!
//! Independent of the main audio engine to avoid interference with
//! transport, recording, and the audio graph.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Plays a decoded audio buffer on a separate cpal output stream.
///
/// The stream stops automatically when playback reaches the end.
/// Drop the `AudioPreview` to release the stream immediately.
pub struct AudioPreview {
    /// Keep the stream alive; dropping stops playback.
    _stream: cpal::Stream,
    /// Current playback frame position (shared with audio callback).
    _position: Arc<AtomicUsize>,
    /// Set to false when playback ends or is stopped.
    playing: Arc<AtomicBool>,
}

impl AudioPreview {
    /// Start playing decoded audio samples on the default output device.
    ///
    /// `samples` is non-interleaved: `[ch0_frame0, ch0_frame1, ..., ch1_frame0, ch1_frame1, ...]`
    /// as produced by `ma_audio_engine::audio_decode::decode_audio_file()`.
    pub fn play(
        samples: Arc<[f32]>,
        channels: usize,
        sample_rate: u32,
        total_frames: usize,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device for preview".to_string())?;

        let config = cpal::StreamConfig {
            channels: channels.min(2) as u16,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let position = Arc::new(AtomicUsize::new(0));
        let playing = Arc::new(AtomicBool::new(true));

        let pos = Arc::clone(&position);
        let play_flag = Arc::clone(&playing);
        let data = Arc::clone(&samples);
        let out_channels = channels.min(2);

        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    // Check if stop() was called externally
                    if !play_flag.load(Ordering::Relaxed) {
                        output.fill(0.0);
                        return;
                    }
                    let mut frame = pos.load(Ordering::Relaxed);
                    for chunk in output.chunks_exact_mut(out_channels) {
                        if frame >= total_frames {
                            play_flag.store(false, Ordering::Relaxed);
                            chunk.fill(0.0);
                        } else {
                            for (ch, sample) in chunk.iter_mut().enumerate() {
                                // Non-interleaved: channel ch starts at ch * total_frames
                                let idx = ch * total_frames + frame;
                                *sample = if idx < data.len() { data[idx] } else { 0.0 };
                            }
                            frame += 1;
                        }
                    }
                    pos.store(frame, Ordering::Relaxed);
                },
                |err| log::error!("Preview stream error: {err}"),
                None,
            )
            .map_err(|e| format!("failed to build preview stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("failed to start preview stream: {e}"))?;

        Ok(Self {
            _stream: stream,
            _position: position,
            playing,
        })
    }

    /// Whether the preview is still playing.
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Stop playback immediately.
    pub fn stop(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }
}
