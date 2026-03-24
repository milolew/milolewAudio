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

/// Maximum consecutive write errors before sending a RecordingError event.
const MAX_CONSECUTIVE_WRITE_ERRORS: u32 = 10;

/// State for a single active recording session.
struct ActiveRecording {
    track_id: TrackId,
    consumer: rtrb::Consumer<f32>,
    writer: WavWriter<std::io::BufWriter<std::fs::File>>,
    _channels: u16,
    total_samples: u64,
    path: PathBuf,
    /// Count of consecutive write errors. Reset on successful write.
    consecutive_write_errors: u32,
    /// Whether a RecordingError event has been sent for this session.
    error_reported: bool,
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
                                consecutive_write_errors: 0,
                                error_reported: false,
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
            let drained = drain_ring_buffer(recording, &mut batch_buffer);
            recording.total_samples += drained as u64;
            if drained > 0 {
                any_data = true;
            }

            // Report persistent write errors to UI (once per session)
            if recording.consecutive_write_errors >= MAX_CONSECUTIVE_WRITE_ERRORS
                && !recording.error_reported
            {
                recording.error_reported = true;
                let _ = evt_tx.send(DiskEvent::RecordingError {
                    track_id: recording.track_id,
                    error: format!(
                        "Persistent disk write errors ({} consecutive failures)",
                        recording.consecutive_write_errors
                    ),
                });
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
/// Tracks consecutive write errors via the `recording` state.
fn drain_ring_buffer(recording: &mut ActiveRecording, batch_buffer: &mut [f32]) -> usize {
    let mut total = 0;

    loop {
        let available = recording.consumer.slots();
        if available == 0 {
            break;
        }

        let to_read = available.min(batch_buffer.len());
        let mut read = 0;

        for sample in batch_buffer.iter_mut().take(to_read) {
            match recording.consumer.pop() {
                Ok(s) => {
                    *sample = s;
                    read += 1;
                }
                Err(_) => break,
            }
        }

        // Write to WAV, tracking consecutive errors
        for &sample in &batch_buffer[..read] {
            if recording.writer.write_sample(sample).is_err() {
                recording.consecutive_write_errors += 1;
                // Don't break immediately — keep draining the ring buffer
                // to avoid backing up the audio thread
            } else {
                recording.consecutive_write_errors = 0;
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
    let remaining = drain_ring_buffer(&mut recording, &mut batch);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_path_format() {
        let track_id = TrackId::new();
        let path = recording_path(Path::new("/tmp/project"), track_id, 3);
        let expected = format!("/tmp/project/recordings/{}_take_003.wav", track_id.0);
        assert_eq!(path.to_str().unwrap(), expected);
    }

    #[test]
    fn disk_io_thread_start_stop() {
        let (cmd_tx, evt_rx) = spawn_disk_io_thread();
        cmd_tx.send(DiskCommand::Shutdown).unwrap();
        // Thread should exit cleanly — verify by dropping the channel
        drop(cmd_tx);
        // Any remaining events should be empty (no active recordings)
        assert!(evt_rx.try_recv().is_err());
    }

    #[test]
    fn disk_io_records_and_finalizes() {
        let (cmd_tx, evt_rx) = spawn_disk_io_thread();
        let track_id = TrackId::new();

        // Create a ring buffer and push some samples
        let (mut producer, consumer) = rtrb::RingBuffer::new(4096);
        let samples: Vec<f32> = (0..100).map(|i| i as f32 * 0.01).collect();
        for &s in &samples {
            producer.push(s).unwrap();
        }

        let output_path = std::env::temp_dir().join(format!("test_record_{}.wav", track_id.0));

        // Start recording
        cmd_tx
            .send(DiskCommand::StartRecording {
                track_id,
                consumer,
                output_path: output_path.clone(),
                channels: 1,
                sample_rate: 48000,
            })
            .unwrap();

        // Give disk thread time to drain
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Stop recording
        cmd_tx
            .send(DiskCommand::StopRecording { track_id })
            .unwrap();

        // Give disk thread time to finalize
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Shutdown
        cmd_tx.send(DiskCommand::Shutdown).unwrap();

        // Check for RecordingComplete event
        let mut found_complete = false;
        while let Ok(event) = evt_rx.try_recv() {
            if let DiskEvent::RecordingComplete {
                track_id: tid,
                total_samples,
                ..
            } = event
            {
                assert_eq!(tid, track_id);
                assert_eq!(total_samples, 100);
                found_complete = true;
            }
        }
        assert!(found_complete, "Expected RecordingComplete event");

        // Verify WAV file exists and has correct sample count
        let reader = hound::WavReader::open(&output_path).unwrap();
        assert_eq!(reader.len(), 100);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 48000);

        // Cleanup
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn disk_io_handles_create_error() {
        let (cmd_tx, evt_rx) = spawn_disk_io_thread();
        let track_id = TrackId::new();
        let (_producer, consumer) = rtrb::RingBuffer::new(1024);

        // Try to write to a non-existent directory
        cmd_tx
            .send(DiskCommand::StartRecording {
                track_id,
                consumer,
                output_path: PathBuf::from("/nonexistent/path/test.wav"),
                channels: 1,
                sample_rate: 48000,
            })
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));
        cmd_tx.send(DiskCommand::Shutdown).unwrap();

        let mut found_error = false;
        while let Ok(event) = evt_rx.try_recv() {
            if let DiskEvent::RecordingError { .. } = event {
                found_error = true;
            }
        }
        assert!(found_error, "Expected RecordingError event for bad path");
    }
}
