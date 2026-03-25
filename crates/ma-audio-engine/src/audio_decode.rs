//! Audio file decoder — decode WAV, FLAC, MP3, OGG to raw f32 samples.
//!
//! Uses symphonia for format-agnostic decoding. Decoded audio is returned as
//! non-interleaved samples matching the `AudioClipRef` format used by `WavPlayerNode`.

use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decoded audio data ready for engine use.
pub struct DecodedAudio {
    /// Non-interleaved samples: channel 0 = `[0..length]`, channel 1 = `[length..2*length]`.
    /// Matches `AudioClipRef::data` layout.
    pub samples: Vec<f32>,
    /// Number of channels.
    pub channels: usize,
    /// Sample rate of the source file.
    pub sample_rate: u32,
    /// Number of samples per channel.
    pub length_samples: usize,
}

/// Errors that can occur during audio decoding.
#[derive(Debug, thiserror::Error)]
pub enum AudioDecodeError {
    /// File could not be opened or read.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Format could not be probed or is unsupported.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    /// No audio track found in the file.
    #[error("no audio track found")]
    NoAudioTrack,
    /// Decoder error during sample extraction.
    #[error("decode error: {0}")]
    Decode(String),
}

/// Decode an audio file to non-interleaved f32 samples.
///
/// Supports WAV, FLAC, MP3, OGG (via symphonia features).
/// Returns audio in non-interleaved layout matching `AudioClipRef::data`.
pub fn decode_audio_file(path: &Path) -> Result<DecodedAudio, AudioDecodeError> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| AudioDecodeError::UnsupportedFormat(e.to_string()))?;

    let mut format_reader = probed.format;

    let track = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or(AudioDecodeError::NoAudioTrack)?;

    let codec_params = track.codec_params.clone();
    let track_id = track.id;

    let channels = codec_params
        .channels
        .map(|c| c.count())
        .ok_or(AudioDecodeError::Decode(
            "missing channel info in codec params".into(),
        ))?;
    let sample_rate = codec_params.sample_rate.ok_or(AudioDecodeError::Decode(
        "missing sample rate in codec params".into(),
    ))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| AudioDecodeError::Decode(e.to_string()))?;

    // Collect interleaved samples first, then convert to non-interleaved
    let mut interleaved: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format_reader.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                break;
            }
            Err(e) => return Err(AudioDecodeError::Decode(e.to_string())),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(AudioDecodeError::Decode(e.to_string())),
        };

        let spec = *decoded.spec();
        let num_frames = decoded.capacity();

        // Reuse SampleBuffer across packets; only reallocate if capacity is insufficient
        let buf = match &mut sample_buf {
            Some(buf) if buf.capacity() >= num_frames => buf,
            _ => {
                sample_buf = Some(SampleBuffer::<f32>::new(num_frames as u64, spec));
                sample_buf.as_mut().unwrap()
            }
        };
        buf.copy_interleaved_ref(decoded);

        interleaved.extend_from_slice(buf.samples());
    }

    let total_frames = interleaved.len() / channels.max(1);

    // Convert interleaved → non-interleaved (planar)
    let mut planar = vec![0.0f32; total_frames * channels];
    for frame in 0..total_frames {
        for ch in 0..channels {
            planar[ch * total_frames + frame] = interleaved[frame * channels + ch];
        }
    }

    Ok(DecodedAudio {
        samples: planar,
        channels,
        sample_rate,
        length_samples: total_frames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test WAV file with a sine wave using hound.
    fn create_test_wav(path: &Path, sample_rate: u32, channels: u16, num_frames: usize) {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for frame in 0..num_frames {
            let t = frame as f32 / sample_rate as f32;
            let sample = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            for _ in 0..channels {
                writer.write_sample(sample).unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn decode_stereo_wav() {
        let path = std::env::temp_dir().join("test_decode_stereo.wav");
        create_test_wav(&path, 48000, 2, 4800);

        let decoded = decode_audio_file(&path).unwrap();

        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.sample_rate, 48000);
        assert_eq!(decoded.length_samples, 4800);
        assert_eq!(decoded.samples.len(), 4800 * 2);

        // Verify non-interleaved layout: ch0 data is in [0..4800], ch1 in [4800..9600]
        // Both channels should have the same sine wave
        for i in 0..100 {
            let ch0 = decoded.samples[i];
            let ch1 = decoded.samples[4800 + i];
            assert!(
                (ch0 - ch1).abs() < 0.001,
                "Channels should be identical for mono-source stereo"
            );
        }

        // Verify non-zero audio
        let rms: f32 = (decoded.samples[..4800].iter().map(|s| s * s).sum::<f32>() / 4800.0).sqrt();
        assert!(rms > 0.1, "Expected non-trivial RMS, got {rms}");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn decode_mono_wav() {
        let path = std::env::temp_dir().join("test_decode_mono.wav");
        create_test_wav(&path, 44100, 1, 2205);

        let decoded = decode_audio_file(&path).unwrap();

        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.length_samples, 2205);
        assert_eq!(decoded.samples.len(), 2205);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn decode_nonexistent_file() {
        let result = decode_audio_file(Path::new("/nonexistent/audio.wav"));
        assert!(matches!(result, Err(AudioDecodeError::Io(_))));
    }
}
