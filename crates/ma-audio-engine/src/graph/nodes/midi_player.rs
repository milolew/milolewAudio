//! MIDI player node — plays MIDI clips on the timeline with a built-in sine synth.
//!
//! This node has 0 inputs and 1 stereo output.
//! MIDI events are read from `MidiClipRef`s and converted to audio using a simple
//! sine wave synthesizer. The synth is a placeholder — future versions will route
//! MIDI to virtual instruments/plugins.
//!
//! # Real-Time Safety
//! - Pre-allocated clips Vec (never grows on audio thread)
//! - Fixed-size `[NoteState; 128]` array (stack-allocated, no heap)
//! - `events_in_range()` uses binary search (no allocations)
//! - No format!, String, Box, or println! in process()

use std::any::Any;
use std::f64::consts::TAU;

use ma_core::audio_buffer::{AudioBuffer, MAX_BUFFER_SIZE};
use ma_core::ids::{ClipId, NodeId};
use ma_core::midi_clip::MidiClipRef;
use ma_core::parameters::{MidiMessage, TransportState};
use ma_core::time;

use crate::graph::node::{AudioNode, ProcessContext};

/// State of a single MIDI note in the built-in synthesizer.
#[derive(Debug, Clone, Copy)]
struct NoteState {
    /// Whether this note is currently sounding.
    active: bool,
    /// Oscillator phase in range [0.0, 1.0).
    phase: f64,
    /// Amplitude derived from velocity (0.0–1.0).
    amplitude: f32,
}

impl Default for NoteState {
    fn default() -> Self {
        Self {
            active: false,
            phase: 0.0,
            amplitude: 0.0,
        }
    }
}

/// Plays MIDI clips using a simple sine wave synthesizer.
///
/// Analogous to `WavPlayerNode` but for MIDI data. Each clip contains
/// timed MIDI events that trigger notes on the built-in synth.
pub struct MidiPlayerNode {
    id: NodeId,
    /// All clips assigned to this player. Pre-allocated, never resized on audio thread.
    clips: Vec<MidiClipRef>,
    /// Maximum number of clips (pre-allocated capacity).
    max_clips: usize,
    /// Per-note synth state. Index = MIDI note number (0–127).
    active_notes: [NoteState; 128],
}

impl MidiPlayerNode {
    /// Create a new MIDI player node with pre-allocated clip capacity.
    pub fn new(id: NodeId, max_clips: usize) -> Self {
        Self {
            id,
            clips: Vec::with_capacity(max_clips),
            max_clips,
            active_notes: [NoteState::default(); 128],
        }
    }

    /// Add a clip to this player. Called from command processor (beginning of audio callback).
    ///
    /// Returns `false` if the clip list is at capacity (no allocation occurs).
    pub fn add_clip(&mut self, clip: MidiClipRef) -> bool {
        if self.clips.len() >= self.max_clips || self.clips.len() >= self.clips.capacity() {
            return false;
        }
        self.clips.push(clip);
        true
    }

    /// Remove a clip by ID. Uses `swap_remove` for O(1) removal.
    ///
    /// # Real-time safety
    /// The removed `MidiClipRef` contains `Arc<MidiClip>`. The project state holds
    /// another Arc reference, so the audio thread will NOT drop the last one.
    pub fn remove_clip(&mut self, clip_id: ClipId) {
        if let Some(pos) = self.clips.iter().position(|c| c.clip_id == clip_id) {
            self.clips.swap_remove(pos);
        }
    }

    /// Convert MIDI note number to frequency (Hz) using A440 equal temperament.
    #[inline]
    fn note_to_freq(note: u8) -> f64 {
        440.0 * f64::powf(2.0, (f64::from(note) - 69.0) / 12.0)
    }

    /// Process MIDI events and synthesize audio for the current buffer.
    fn render_clips(&mut self, output: &mut AudioBuffer, context: &ProcessContext) {
        output.clear();

        if context.transport_state == TransportState::Stopped {
            // Clear all active notes on stop
            for note in &mut self.active_notes {
                note.active = false;
            }
            return;
        }

        let tempo = context.tempo;
        let sample_rate = f64::from(context.sample_rate);
        let buffer_size = context.buffer_size;

        // Convert playhead range to ticks
        let playhead = context.playhead_samples;
        let buffer_end = playhead + i64::from(buffer_size);

        let Some(start_tick) = time::samples_to_ticks(playhead, tempo, sample_rate) else {
            return;
        };
        let Some(end_tick) = time::samples_to_ticks(buffer_end, tempo, sample_rate) else {
            return;
        };

        // Process MIDI events from all clips
        for clip in &self.clips {
            let clip_end_tick = clip.start_tick + clip.clip.duration_ticks();

            // Skip clips that don't overlap with this buffer
            if end_tick <= clip.start_tick || start_tick >= clip_end_tick {
                continue;
            }

            // Convert to clip-local tick range
            let local_start = (start_tick - clip.start_tick).max(0);
            let local_end = (end_tick - clip.start_tick).min(clip.clip.duration_ticks());

            let events = clip.clip.events_in_range(local_start, local_end);

            for event in events {
                match event.message {
                    MidiMessage::NoteOn { note, velocity, .. } => {
                        let idx = note as usize;
                        if idx < 128 {
                            // MIDI spec: NoteOn with velocity 0 is equivalent to NoteOff
                            if velocity == 0 {
                                self.active_notes[idx].active = false;
                            } else {
                                self.active_notes[idx].active = true;
                                self.active_notes[idx].amplitude = f32::from(velocity) / 127.0;
                            }
                        }
                    }
                    MidiMessage::NoteOff { note, .. } => {
                        let idx = note as usize;
                        if idx < 128 {
                            self.active_notes[idx].active = false;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Synthesize audio from active notes into a stack buffer,
        // then copy to both channels. Avoids per-sample bounds checks
        // from get_mut() and the double-borrow issue with channel_mut().
        let frames = buffer_size as usize;
        let inv_sr = 1.0 / sample_rate;
        let mut temp = [0.0f32; MAX_BUFFER_SIZE];

        for note_num in 0..128u8 {
            let state = &mut self.active_notes[note_num as usize];
            if !state.active {
                continue;
            }

            let freq = Self::note_to_freq(note_num);
            let phase_inc = freq * inv_sr;
            let amp = state.amplitude;

            for sample in temp.iter_mut().take(frames) {
                *sample += (state.phase * TAU).sin() as f32 * amp * 0.25;
                state.phase += phase_inc;
                if state.phase >= 1.0 {
                    state.phase -= 1.0;
                }
            }
        }

        // Copy synthesized audio to both stereo channels
        {
            let ch = output.channel_mut(0);
            for (dst, src) in ch.iter_mut().zip(temp.iter()) {
                *dst += src;
            }
        }
        {
            let ch = output.channel_mut(1);
            for (dst, src) in ch.iter_mut().zip(temp.iter()) {
                *dst += src;
            }
        }
    }
}

impl AudioNode for MidiPlayerNode {
    fn process(
        &mut self,
        _inputs: &[&AudioBuffer],
        outputs: &mut [&mut AudioBuffer],
        context: &ProcessContext,
    ) {
        if let Some(output) = outputs.first_mut() {
            self.render_clips(output, context);
        }
    }

    fn input_count(&self) -> usize {
        0
    }

    fn output_count(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        for note in &mut self.active_notes {
            *note = NoteState::default();
        }
    }

    fn node_id(&self) -> NodeId {
        self.id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ma_core::ids::{ClipId, NodeId};
    use ma_core::midi_clip::{MidiClip, MidiClipRef};
    use ma_core::parameters::{MidiEvent, MidiMessage, TransportState};
    use ma_core::time::PPQN;

    use crate::graph::node::{AudioNode, ProcessContext};

    use super::MidiPlayerNode;

    fn make_context(
        transport_state: TransportState,
        playhead_samples: i64,
        tempo: f64,
    ) -> ProcessContext {
        ProcessContext {
            sample_rate: 48000.0,
            transport_state,
            playhead_samples,
            tempo,
            buffer_size: 256,
            any_solo: false,
        }
    }

    fn note_on(tick: i64, note: u8, velocity: u8) -> MidiEvent {
        MidiEvent {
            tick,
            message: MidiMessage::NoteOn {
                channel: 0,
                note,
                velocity,
            },
        }
    }

    fn note_off(tick: i64, note: u8) -> MidiEvent {
        MidiEvent {
            tick,
            message: MidiMessage::NoteOff {
                channel: 0,
                note,
                velocity: 0,
            },
        }
    }

    fn make_clip_ref(events: Vec<MidiEvent>, duration: i64, start_tick: i64) -> MidiClipRef {
        MidiClipRef {
            clip_id: ClipId::new(),
            clip: Arc::new(MidiClip::new(events, duration)),
            start_tick,
        }
    }

    #[test]
    fn add_clip_respects_capacity() {
        let mut node = MidiPlayerNode::new(NodeId(1), 2);

        let clip1 = make_clip_ref(vec![], 960, 0);
        let clip2 = make_clip_ref(vec![], 960, 0);
        let clip3 = make_clip_ref(vec![], 960, 0);

        assert!(node.add_clip(clip1));
        assert!(node.add_clip(clip2));
        assert!(!node.add_clip(clip3)); // At capacity
    }

    #[test]
    fn remove_clip_by_id() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);
        let id = ClipId::new();

        let clip = MidiClipRef {
            clip_id: id,
            clip: Arc::new(MidiClip::new(vec![], 960)),
            start_tick: 0,
        };
        node.add_clip(clip);
        assert_eq!(node.clips.len(), 1);

        node.remove_clip(id);
        assert_eq!(node.clips.len(), 0);
    }

    #[test]
    fn remove_nonexistent_clip_is_noop() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);
        node.remove_clip(ClipId::new()); // Should not panic
        assert_eq!(node.clips.len(), 0);
    }

    #[test]
    fn stopped_transport_outputs_silence() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);

        // Add clip with a note
        let clip = make_clip_ref(vec![note_on(0, 60, 100)], PPQN * 4, 0);
        node.add_clip(clip);

        let mut buf = ma_core::AudioBuffer::stereo(256);
        let context = make_context(TransportState::Stopped, 0, 120.0);

        let mut outputs: Vec<&mut ma_core::AudioBuffer> = vec![&mut buf];
        node.process(&[], &mut outputs, &context);

        // Output should be silence
        for frame in 0..256 {
            assert_eq!(*buf.get(0, frame).unwrap(), 0.0);
            assert_eq!(*buf.get(1, frame).unwrap(), 0.0);
        }
    }

    #[test]
    fn playing_with_note_produces_audio() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);

        // Clip at tick 0 with a note on at tick 0
        let clip = make_clip_ref(
            vec![note_on(0, 69, 127)], // A4 = 440Hz
            PPQN * 4,
            0,
        );
        node.add_clip(clip);

        let mut buf = ma_core::AudioBuffer::stereo(256);
        // At 120 BPM, 48kHz: playhead=0 maps to tick=0
        let context = make_context(TransportState::Playing, 0, 120.0);

        let mut outputs: Vec<&mut ma_core::AudioBuffer> = vec![&mut buf];
        node.process(&[], &mut outputs, &context);

        // Output should contain non-zero samples (sine wave)
        let has_nonzero = (0..256).any(|f| buf.get(0, f).unwrap().abs() > 0.001);
        assert!(
            has_nonzero,
            "Expected non-zero audio output for active note"
        );
    }

    #[test]
    fn note_off_silences_note() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);

        // Note on at tick 0, note off at tick 1
        let clip = make_clip_ref(vec![note_on(0, 69, 127), note_off(1, 69)], PPQN * 4, 0);
        node.add_clip(clip);

        let mut buf = ma_core::AudioBuffer::stereo(256);
        let context = make_context(TransportState::Playing, 0, 120.0);

        let mut outputs: Vec<&mut ma_core::AudioBuffer> = vec![&mut buf];
        node.process(&[], &mut outputs, &context);

        // After processing, note 69 should be inactive
        assert!(!node.active_notes[69].active);
    }

    #[test]
    fn clip_outside_buffer_produces_silence() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);

        // Clip starts at tick 96000 (far ahead)
        let clip = make_clip_ref(vec![note_on(0, 60, 100)], PPQN, 96000);
        node.add_clip(clip);

        let mut buf = ma_core::AudioBuffer::stereo(256);
        let context = make_context(TransportState::Playing, 0, 120.0);

        let mut outputs: Vec<&mut ma_core::AudioBuffer> = vec![&mut buf];
        node.process(&[], &mut outputs, &context);

        for frame in 0..256 {
            assert_eq!(*buf.get(0, frame).unwrap(), 0.0);
        }
    }

    #[test]
    fn reset_clears_all_notes() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);

        // Activate a note manually
        node.active_notes[60].active = true;
        node.active_notes[60].amplitude = 1.0;

        node.reset();

        assert!(!node.active_notes[60].active);
        assert_eq!(node.active_notes[60].amplitude, 0.0);
    }

    #[test]
    fn node_id_is_correct() {
        let node = MidiPlayerNode::new(NodeId(42), 4);
        assert_eq!(node.node_id(), NodeId(42));
    }

    #[test]
    fn input_output_counts() {
        let node = MidiPlayerNode::new(NodeId(1), 4);
        assert_eq!(node.input_count(), 0);
        assert_eq!(node.output_count(), 1);
    }

    #[test]
    fn note_to_freq_a4() {
        let freq = MidiPlayerNode::note_to_freq(69);
        assert!((freq - 440.0).abs() < 0.01);
    }

    #[test]
    fn note_to_freq_c4() {
        let freq = MidiPlayerNode::note_to_freq(60);
        assert!((freq - 261.63).abs() < 0.1);
    }

    #[test]
    fn multiple_notes_mix_together() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);

        // Two notes at tick 0
        let clip = make_clip_ref(vec![note_on(0, 60, 100), note_on(0, 64, 100)], PPQN * 4, 0);
        node.add_clip(clip);

        let mut buf = ma_core::AudioBuffer::stereo(256);
        let context = make_context(TransportState::Playing, 0, 120.0);

        let mut outputs: Vec<&mut ma_core::AudioBuffer> = vec![&mut buf];
        node.process(&[], &mut outputs, &context);

        // Both notes should be active
        assert!(node.active_notes[60].active);
        assert!(node.active_notes[64].active);
    }

    #[test]
    fn as_any_downcast_works() {
        let node = MidiPlayerNode::new(NodeId(1), 4);
        let any_ref = node.as_any();
        assert!(any_ref.downcast_ref::<MidiPlayerNode>().is_some());
    }

    #[test]
    fn note_on_velocity_zero_treated_as_note_off() {
        let mut node = MidiPlayerNode::new(NodeId(1), 4);

        // NoteOn with velocity 0 should deactivate the note (MIDI spec)
        let clip = make_clip_ref(vec![note_on(0, 60, 100), note_on(1, 60, 0)], PPQN * 4, 0);
        node.add_clip(clip);

        let mut buf = ma_core::AudioBuffer::stereo(256);
        let context = make_context(TransportState::Playing, 0, 120.0);

        let mut outputs: Vec<&mut ma_core::AudioBuffer> = vec![&mut buf];
        node.process(&[], &mut outputs, &context);

        assert!(!node.active_notes[60].active);
    }
}
