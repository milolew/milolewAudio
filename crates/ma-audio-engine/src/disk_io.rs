//! Disk I/O thread — reads recorded audio from ring buffers and writes to WAV files.
//!
//! This runs on a dedicated thread (NOT the audio thread). It:
//! - Owns the consumer side of each track's recording ring buffer
//! - Batch-reads samples and writes them to WAV files via hound
//! - Responds to start/stop recording signals via crossbeam channel

use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};
use hound::{SampleFormat, WavSpec, WavWriter};

use ma_core::ids::TrackId;

/// Commands sent to the disk I/O thread.
pub enum DiskCommand {
    /// Start recording for a track. Provides the ring buffer consumer and output path.
    StartRecording {
        track_id: TrackId,
        consumer: rtrb::Consumer<f32>,
        output_path: PathBuf,
        channels: u16,
        sample_rate: u32,
    },

    /// Stop recording for a track. Finalizes the WAV file.
    StopRecording { track_id: TrackId },

    /// Shut down the disk I/O thread.
    Shutdown,
}

/// Events sent back from the disk I/O thread.
pub enum DiskEvent {
    /// Recording has been finalized and saved to disk.
    RecordingComplete {
        track_id: TrackId,
        path: PathBuf,
        total_samples: u64,
    },

    /// An error occurred during recording.
    RecordingError { track_id: TrackId, error: String },
}

/// State for a single active recording session.
struct ActiveRecording {
    track_id: TrackId,
    consumer: rtrb::Consumer<f32>,
    writer: WavWriter<std::io::BufWriter<std::fs::File>>,
    _channels: u16,
    total_samples: u64,
    path: PathBuf,
}

/// Batch size for reading from ring buffer.
/// ~4096 frames × 2 channels = 8192 samples per batch.
const BATCH_SIZE: usize = 8192;

/// Create the disk I/O thread communication channels.
///
/// Returns (command_sender, event_receiver) for the caller,
/// and spawns the disk I/O thread internally.
pub fn spawn_disk_io_thread() -> (Sender<DiskCommand>, Receiver<DiskEvent>) {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<DiskCommand>();
    let (evt_tx, evt_rx) = crossbeam_channel::unbounded::<DiskEvent>();

    std::thread::Builder::new()
        .name("disk-io".into())
        .spawn(move || {
            disk_io_loop(cmd_rx, evt_tx);
        })
        .expect("Failed to spawn disk I/O thread");

    (cmd_tx, evt_rx)
}

/// Main loop for the disk I/O thread.
fn disk_io_loop(cmd_rx: Receiver<DiskCommand>, evt_tx: Sender<DiskEvent>) {
    let mut active_recordings: Vec<ActiveRecording> = Vec::new();
    let mut batch_buffer = vec![0.0f32; BATCH_SIZE];

    loop {
        // Process commands (non-blocking)
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                DiskCommand::StartRecording {
                    track_id,
                    consumer,
                    output_path,
                    channels,
                    sample_rate,
                } => {
                    let spec = WavSpec {
                        channels,
                        sample_rate,
                        bits_per_sample: 32,
                        sample_format: SampleFormat::Float,
                    };

                    match WavWriter::create(&output_path, spec) {
                        Ok(writer) => {
                            active_recordings.push(ActiveRecording {
                                track_id,
                                consumer,
                                writer,
                                _channels: channels,
                                total_samples: 0,
                                path: output_path,
                            });
                        }
                        Err(e) => {
                            let _ = evt_tx.send(DiskEvent::RecordingError {
                                track_id,
                                error: format!("Failed to create WAV file: {}", e),
                            });
                        }
                    }
                }

                DiskCommand::StopRecording { track_id } => {
                    // Find and finalize this recording
                    if let Some(pos) = active_recordings
                        .iter()
                        .position(|r| r.track_id == track_id)
                    {
                        let recording = active_recordings.remove(pos);
                        finalize_recording(recording, &evt_tx);
                    }
                }

                DiskCommand::Shutdown => {
                    // Finalize all active recordings
                    for recording in active_recordings.drain(..) {
                        finalize_recording(recording, &evt_tx);
                    }
                    return;
                }
            }
        }

        // Drain audio from all active recordings' ring buffers
        let mut any_data = false;
        for recording in &mut active_recordings {
            let drained = drain_ring_buffer(
                &mut recording.consumer,
                &mut batch_buffer,
                &mut recording.writer,
            );
            recording.total_samples += drained as u64;
            if drained > 0 {
                any_data = true;
            }
        }

        // If no data was available, sleep briefly to avoid busy-waiting.
        // At 48kHz stereo with 256-frame buffers, new data arrives every ~5.3ms.
        if !any_data {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

/// Drain samples from a ring buffer and write them to the WAV file.
/// Returns the number of samples written.
fn drain_ring_buffer(
    consumer: &mut rtrb::Consumer<f32>,
    batch_buffer: &mut [f32],
    writer: &mut WavWriter<std::io::BufWriter<std::fs::File>>,
) -> usize {
    let mut total = 0;

    loop {
        let available = consumer.slots();
        if available == 0 {
            break;
        }

        let to_read = available.min(batch_buffer.len());
        let mut read = 0;

        for sample in batch_buffer.iter_mut().take(to_read) {
            match consumer.pop() {
                Ok(s) => {
                    *sample = s;
                    read += 1;
                }
                Err(_) => break,
            }
        }

        // Write to WAV
        for &sample in &batch_buffer[..read] {
            if writer.write_sample(sample).is_err() {
                // Disk error — we lose this sample but continue trying
                break;
            }
        }

        total += read;

        if read < to_read {
            break;
        }
    }

    total
}

/// Finalize a recording: drain remaining samples, finalize WAV header.
fn finalize_recording(mut recording: ActiveRecording, evt_tx: &Sender<DiskEvent>) {
    // Drain any remaining samples
    let mut batch = vec![0.0f32; BATCH_SIZE];
    let remaining = drain_ring_buffer(&mut recording.consumer, &mut batch, &mut recording.writer);
    recording.total_samples += remaining as u64;

    // Finalize WAV header (updates the data chunk size)
    match recording.writer.finalize() {
        Ok(_) => {
            let _ = evt_tx.send(DiskEvent::RecordingComplete {
                track_id: recording.track_id,
                path: recording.path,
                total_samples: recording.total_samples,
            });
        }
        Err(e) => {
            let _ = evt_tx.send(DiskEvent::RecordingError {
                track_id: recording.track_id,
                error: format!("Failed to finalize WAV: {}", e),
            });
        }
    }
}

/// Utility: generate a recording file path.
pub fn recording_path(project_dir: &Path, track_id: TrackId, take_number: u32) -> PathBuf {
    project_dir
        .join("recordings")
        .join(format!("{}_take_{:03}.wav", track_id.0, take_number))
}
