# milolew Audio — CLAUDE.md

> Open-source DAW (Digital Audio Workstation) inspired by Ableton Live.
> Built in Rust with egui, cpal, and custom audio engine.
> **This file is the single source of truth for Claude Code working on this project.**

---

## 🎯 Project Vision

milolew Audio is a cross-platform, open-source DAW focused on:
1. **Low-latency audio recording** — ASIO (Windows), CoreAudio (macOS), JACK/ALSA (Linux)
2. **MIDI sequencing** with built-in synthesizers and instruments
3. **VST3 plugin hosting** — scanning, loading, parameter control, GUI hosting
4. **Arrangement View** (timeline) first, Session View (clip launcher) later
5. **Modern Rust architecture** — memory-safe, lock-free, parallel processing

License: **GPLv3**

---

## 🏗️ Architecture Overview

```
milolew-audio/
├── CLAUDE.md                    # ← You are here
├── Cargo.toml                   # Workspace root
├── crates/
│   ├── ma-core/                 # Shared types, time units, IDs, errors
│   │   └── CLAUDE.md
│   ├── ma-audio-engine/         # Real-time audio graph, transport, mixing
│   │   └── CLAUDE.md
│   ├── ma-dsp/                  # DSP primitives: filters, oscillators, envelopes
│   │   └── CLAUDE.md
│   ├── ma-midi/                 # MIDI I/O, sequencer, event scheduling
│   │   └── CLAUDE.md
│   ├── ma-synth/                # Built-in synthesizers (wavetable, subtractive, FM)
│   │   └── CLAUDE.md
│   ├── ma-plugin-host/          # VST3 plugin scanning, loading, hosting
│   │   └── CLAUDE.md
│   ├── ma-project/              # Project file format, state management, undo/redo
│   │   └── CLAUDE.md
│   ├── ma-ui/                   # egui-based GUI: timeline, piano roll, mixer
│   │   └── CLAUDE.md
│   └── ma-app/                  # Binary entry point, app lifecycle, window management
│       └── CLAUDE.md
├── assets/                      # Fonts, icons, default presets, wavetables
├── tests/                       # Integration tests
├── benches/                     # Criterion benchmarks (DSP, audio graph)
└── docs/                        # Architecture diagrams, design decisions
    └── architecture.md
```

### Crate Dependency Graph (strict layering — no circular deps)

```
ma-app
  ├── ma-ui
  │     ├── ma-project
  │     │     ├── ma-audio-engine
  │     │     │     ├── ma-dsp
  │     │     │     │     └── ma-core
  │     │     │     ├── ma-midi
  │     │     │     │     └── ma-core
  │     │     │     ├── ma-synth
  │     │     │     │     ├── ma-dsp
  │     │     │     │     └── ma-core
  │     │     │     ├── ma-plugin-host
  │     │     │     │     └── ma-core
  │     │     │     └── ma-core
  │     │     └── ma-core
  │     └── ma-core
  └── ma-core
```

**Rule: lower crates NEVER depend on higher crates. Audio engine NEVER depends on UI.**

---

## 🔧 Build & Run Commands

```bash
# Build entire workspace
cargo build --workspace

# Build in release mode (ALWAYS for audio performance testing)
cargo build --workspace --release

# Run the application
cargo run -p ma-app --release

# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p ma-audio-engine

# Run benchmarks
cargo bench --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --all

# Check ASIO support on Windows (feature flag)
cargo build -p ma-audio-engine --features asio
```

### CI Must Pass
- `cargo test --workspace` — all tests green
- `cargo clippy --workspace -- -D warnings` — zero warnings
- `cargo fmt --all -- --check` — formatted
- `cargo bench --workspace` — no regressions >5%

---

## 🧵 REAL-TIME AUDIO THREAD RULES (CRITICAL)

**These rules are NON-NEGOTIABLE. Violating any of them causes audio glitches (clicks, pops, dropouts). Claude Code MUST enforce these in every code review and generation.**

### ❌ NEVER do these on the audio thread (`AudioCallback`, `process()`, any `fn` called from audio callback):

1. **NEVER allocate heap memory** — no `Vec::push`, `Box::new`, `String::new`, `format!()`, `HashMap::insert` that triggers resize
2. **NEVER lock a Mutex/RwLock** — use only lock-free structures (`AtomicXxx`, `ringbuf`, `crossbeam` channels)
3. **NEVER do file/disk I/O** — no `std::fs`, no `File::open`, no logging to disk
4. **NEVER call system functions with unbounded time** — no `thread::sleep`, no `println!()`, no network
5. **NEVER deallocate on audio thread** — drop large objects on a dedicated GC thread
6. **NEVER use `Rc`/`Arc` with refcount changes** — atomic refcount ops can contend
7. **NEVER panic** — use `Result` types, handle errors gracefully, return silence on error

### ✅ ALWAYS do this on the audio thread:

1. **Pre-allocate all buffers** at initialization, reuse them
2. **Use `AtomicF32`/`AtomicBool`/`AtomicU64`** for parameter changes from UI
3. **Use lock-free SPSC ring buffers** for commands (UI→Engine) and events (Engine→UI)
4. **Use `#[inline]` on hot DSP functions** — audio callback must complete within buffer period
5. **Process audio in-place** when possible to minimize buffer copies
6. **Return silence** (`0.0f32`) on any error — never propagate panics

### Thread Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  UI Thread (egui main loop)                                  │
│  - Renders GUI at vsync (60fps)                              │
│  - Sends commands via lock-free SPSC queue                   │
│  - Reads meter/state values via AtomicF32 / ring buffer      │
└──────┬────────────────────────────────────▲───────────────────┘
       │ CommandRingBuffer                  │ MeterRingBuffer
       │ (play, stop, set_param, etc.)      │ (peak levels, position, etc.)
┌──────▼────────────────────────────────────┴───────────────────┐
│  Audio Thread (cpal callback, ~5ms deadline @ 256 samples)    │
│  - Reads commands from ring buffer                            │
│  - Processes audio graph (DAG topological order)              │
│  - Runs MIDI sequencer                                        │
│  - Hosts VST3 plugins                                         │
│  - Writes meter data to ring buffer                           │
└──────┬────────────────────────────────────────────────────────┘
       │
┌──────▼────────────────────────────────────────────────────────┐
│  Worker Threads (rayon / dedicated threads)                    │
│  - Disk streaming (audio file read-ahead)                     │
│  - Audio recording (write-behind)                             │
│  - VST3 plugin scanning                                       │
│  - Waveform peak computation                                  │
│  - Offline bounce/render                                      │
└───────────────────────────────────────────────────────────────┘
```

---

## 📦 Key Dependencies

```toml
# Cargo.toml (workspace)
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
# Audio I/O
cpal = "0.15"                    # Cross-platform audio (ASIO via feature flag)

# MIDI
midir = "0.10"                   # Cross-platform MIDI I/O

# GUI
eframe = "0.31"                  # egui framework with wgpu backend
egui = "0.31"                    # Immediate-mode GUI
egui_extras = "0.31"             # Additional widgets

# Lock-free communication
ringbuf = "0.4"                  # SPSC ring buffer (audio ↔ UI)
crossbeam-channel = "0.5"        # MPMC for worker threads (non-RT only)

# Audio file I/O
hound = "3.5"                    # WAV reading/writing
symphonia = "0.5"                # Multi-format audio decoding (FLAC, MP3, OGG, WAV)

# DSP
dasp = "0.11"                    # Digital audio signal processing primitives
rustfft = "6"                    # FFT for spectrum analysis

# Plugin hosting (VST3)
# vst3-sys = "0.3"              # Raw VST3 bindings (low-level, evaluate first)
# OR custom wrapper — see ma-plugin-host/CLAUDE.md

# Utilities
serde = { version = "1", features = ["derive"] }  # Serialization
serde_json = "1"                 # Project file format
uuid = { version = "1", features = ["v4"] }        # Unique IDs for tracks, clips
log = "0.4"                      # Logging facade
env_logger = "0.11"              # Logger implementation
thiserror = "2"                  # Error types
anyhow = "1"                     # Error handling in app layer
rayon = "1.10"                   # Parallel processing for offline tasks

# Testing & benchmarks
criterion = { version = "0.5", features = ["html_reports"] }

[workspace.dependencies.cpal]
version = "0.15"
# On Windows with ASIO:
# features = ["asio"]
```

---

## 🎵 Core Data Model

### Time Representation (ma-core)

```rust
/// All time in the project exists in two domains:
/// - Musical time: Ticks (960 PPQN — pulses per quarter note)
/// - Absolute time: Samples (at project sample rate, e.g., 44100 or 48000)
///
/// The Transport is the ONLY place that converts between them using current tempo.
/// UI displays Bars:Beats:Ticks. Audio engine works in samples.

pub type Tick = i64;          // Musical time (960 per quarter note)
pub type SamplePos = i64;     // Absolute sample position
pub type FrameCount = u32;    // Buffer frame count (per callback)

pub const PPQN: Tick = 960;   // Pulses Per Quarter Note
```

### Project Hierarchy

```
Project
├── name: String
├── sample_rate: u32           (44100 | 48000 | 96000)
├── tempo: f64                 (BPM, default 120.0)
├── time_signature: (u8, u8)   (numerator, denominator)
├── master_bus: Bus
├── tracks: Vec<Track>
│   ├── AudioTrack
│   │   ├── clips: Vec<AudioClip>
│   │   │   ├── source_file: PathBuf
│   │   │   ├── start_tick: Tick
│   │   │   ├── duration_ticks: Tick
│   │   │   ├── gain: f32
│   │   │   └── fade_in / fade_out
│   │   ├── plugins: Vec<PluginInstance>
│   │   ├── volume: AtomicF32
│   │   ├── pan: AtomicF32
│   │   ├── mute: AtomicBool
│   │   └── solo: AtomicBool
│   └── MidiTrack
│       ├── clips: Vec<MidiClip>
│       │   ├── events: Vec<MidiEvent>    {tick, message, velocity, channel}
│       │   ├── start_tick: Tick
│       │   └── duration_ticks: Tick
│       ├── instrument: InstrumentSlot    (built-in synth OR VST3)
│       ├── plugins: Vec<PluginInstance>   (effects chain)
│       └── ... (volume, pan, mute, solo)
└── undo_stack: UndoStack
```

### Audio Graph (DAG)

```
                    ┌─────────┐
                    │ Master  │ → Audio Output (cpal)
                    │  Bus    │
                    └────▲────┘
                         │ mix
              ┌──────────┼──────────┐
              │          │          │
         ┌────┴───┐ ┌───┴────┐ ┌──┴──────┐
         │Track 1 │ │Track 2 │ │Track 3  │
         │(Audio) │ │(MIDI)  │ │(Audio)  │
         └────▲───┘ └───▲────┘ └────▲────┘
              │         │           │
         ┌────┴───┐ ┌───┴────┐ ┌───┴─────┐
         │ Clip   │ │ Synth  │ │ Clip    │
         │ Player │ │ Engine │ │ Player  │
         └────────┘ └────────┘ └─────────┘

Processing order: topological sort, leaves → root.
Independent branches can process in parallel (future optimization).
```

---

## 🎹 MIDI Architecture (ma-midi)

```rust
/// MIDI event with tick-accurate timing
pub struct MidiEvent {
    pub tick: Tick,
    pub message: MidiMessage,
}

pub enum MidiMessage {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    PitchBend { channel: u8, value: i16 },
    ProgramChange { channel: u8, program: u8 },
}

/// Piano roll note representation (for UI and editing)
pub struct Note {
    pub id: Uuid,
    pub pitch: u8,          // 0-127 (MIDI note number)
    pub start_tick: Tick,
    pub duration_ticks: Tick,
    pub velocity: u8,       // 0-127
    pub channel: u8,        // 0-15
}
```

**Sequencer rules:**
- Tempo-mapped: events stored in ticks, converted to samples at playback via transport
- Record quantize: snap to grid on input (configurable: off, 1/4, 1/8, 1/16, 1/32)
- Per-callback: calculate tick range for buffer, collect events, schedule with sample-accurate offset

---

## 🎛️ Built-in Synthesizers (ma-synth)

### Phase 1 — Wavetable Synth
- Single-cycle waveforms (1024 samples): sine, saw, square, triangle
- PolyBLEP anti-aliasing for saw/square
- 16-voice polyphony (voice stealing: oldest note)
- ADSR envelope (attack, decay, sustain, release) with exponential curves
- Low-pass SVF filter (State Variable Filter) with cutoff + resonance
- LFO → pitch, filter cutoff, amplitude

### Phase 2 — Subtractive Synth
- 2 oscillators + sub-oscillator + noise
- Oscillator detune, mix
- Multi-mode filter (LP, HP, BP, Notch)
- 2 ADSR envelopes (amp + filter)
- Modulation matrix

### Phase 3 — FM Synth
- 4-operator FM with selectable algorithms
- Per-operator: frequency ratio, detune, ADSR, output level
- Feedback on operator 4

**All synths implement the same trait:**
```rust
pub trait Instrument: Send {
    fn process_midi(&mut self, event: &MidiEvent);
    fn render(&mut self, buffer: &mut [f32], num_frames: u32, sample_rate: f32);
    fn reset(&mut self);
    fn voice_count(&self) -> usize;
    fn set_parameter(&mut self, id: u32, value: f32);
    fn get_parameter(&self, id: u32) -> f32;
}
```

---

## 🔌 VST3 Plugin Hosting (ma-plugin-host)

Strategy: Start with `vst3-sys` raw bindings, build safe Rust wrapper layer.

```rust
pub trait PluginHost {
    fn scan_directory(&mut self, path: &Path) -> Vec<PluginInfo>;
    fn load_plugin(&mut self, info: &PluginInfo) -> Result<PluginInstance>;
    fn unload_plugin(&mut self, instance: PluginInstance);
}

pub struct PluginInstance {
    pub id: Uuid,
    pub info: PluginInfo,
    // ... VST3 component references
}

impl PluginInstance {
    /// Called on audio thread — MUST be RT-safe
    pub fn process(&mut self, audio: &mut AudioBuffer, midi: &[MidiEvent]) { ... }

    /// Called on UI thread
    pub fn open_editor(&mut self, parent_window: RawWindowHandle) { ... }
    pub fn close_editor(&mut self) { ... }

    pub fn get_parameter(&self, id: u32) -> f32 { ... }
    pub fn set_parameter(&mut self, id: u32, value: f32) { ... }
}
```

**VST3 hosting constraints:**
- Plugin scanning ALWAYS on worker thread (plugins can crash, use separate process if possible)
- Plugin GUI runs on UI thread (VST3 requires it)
- Audio processing on audio thread via `IAudioProcessor::process()`
- Parameter changes: queue from UI, apply on audio thread before processing

---

## 🖥️ GUI Architecture (ma-ui, egui)

### Main Views
1. **Toolbar** — transport controls, tempo, time signature, CPU meter
2. **Arrangement View** — horizontal timeline with track lanes, clips, playhead
3. **Piano Roll** — MIDI note editor (opens per-clip)
4. **Mixer** — vertical channel strips with faders, pan, meters, plugin slots
5. **Browser** — file browser for audio files, presets, plugins

### egui Custom Widget Pattern

```rust
/// All custom DAW widgets follow this pattern:
pub struct TimelineWidget<'a> {
    state: &'a mut TimelineState,
    project: &'a Project,
}

impl<'a> TimelineWidget<'a> {
    pub fn show(self, ui: &mut egui::Ui) -> TimelineResponse {
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::click_and_drag(),
        );

        if ui.is_rect_visible(rect) {
            self.paint_background(ui, rect);
            self.paint_tracks(ui, rect);
            self.paint_clips(ui, rect);
            self.paint_playhead(ui, rect);
        }

        self.handle_input(&response);
        TimelineResponse { /* ... */ }
    }
}
```

### Waveform Rendering
- Pre-compute peak mipmaps on import (worker thread): resolutions 256x, 64x, 16x, 4x, 1x
- Store as `Vec<(f32, f32)>` (min, max) per mipmap level
- Select mipmap based on current zoom level
- Render via `egui::Painter::line_segment()` or custom `epaint::Shape`
- NEVER compute peaks on audio thread or UI thread during playback

### UI ↔ Engine Communication

```rust
/// Commands sent from UI to audio engine (via SPSC ring buffer)
pub enum EngineCommand {
    Play,
    Stop,
    Pause,
    SetPosition(Tick),
    SetTempo(f64),
    SetTrackVolume { track_id: Uuid, volume: f32 },
    SetTrackPan { track_id: Uuid, pan: f32 },
    SetTrackMute { track_id: Uuid, mute: bool },
    SetTrackSolo { track_id: Uuid, solo: bool },
    SetParameter { plugin_id: Uuid, param_id: u32, value: f32 },
    AddTrack(TrackConfig),
    RemoveTrack(Uuid),
    // ... more commands
}

/// State sent from audio engine to UI (via SPSC ring buffer or atomics)
pub struct EngineState {
    pub playhead_position: AtomicI64,   // Current position in ticks
    pub is_playing: AtomicBool,
    pub is_recording: AtomicBool,
    pub cpu_load: AtomicF32,            // Audio thread CPU usage (0.0 - 1.0)
    pub peak_meters: Vec<AtomicF32>,    // Per-track peak levels
}
```

---

## 📁 Project File Format (ma-project)

```json
{
  "milolew_audio_version": "0.1.0",
  "project": {
    "name": "My Song",
    "sample_rate": 48000,
    "tempo": 120.0,
    "time_signature": [4, 4],
    "tracks": [
      {
        "id": "uuid-...",
        "type": "audio",
        "name": "Vocals",
        "clips": [...],
        "plugins": [...],
        "volume": 0.8,
        "pan": 0.0,
        "mute": false,
        "solo": false
      }
    ]
  }
}
```

- Format: JSON (human-readable, diffable, serde-native)
- Audio files: referenced by relative path from project directory
- Plugin state: serialized as base64 binary blob (VST3 `getState()`)
- Auto-save: every 60 seconds to `.autosave` file

---

## ↩️ Undo/Redo System (Command Pattern)

```rust
pub trait UndoableCommand: Send {
    fn execute(&mut self, project: &mut Project);
    fn undo(&mut self, project: &mut Project);
    fn description(&self) -> &str;
}

pub struct CompoundCommand {
    commands: Vec<Box<dyn UndoableCommand>>,
    description: String,
}

pub struct UndoStack {
    undo_stack: Vec<Box<dyn UndoableCommand>>,
    redo_stack: Vec<Box<dyn UndoableCommand>>,
    max_depth: usize,  // default: 200
}
```

**Rules:**
- Every user action creates a command (move clip, change volume, add track, etc.)
- Drag operations: defer command creation until mouse release
- Group related changes into `CompoundCommand` (e.g., multi-clip move)
- Reference objects by `Uuid`, NEVER by index or pointer
- Undo stack lives on UI thread only

---

## 🧪 Testing Strategy

### Unit Tests
- Every DSP function: compare output against reference values (±1e-6 tolerance)
- Every synth: test note on/off, verify non-silence, verify silence after release
- Lock-free structures: test under contention with `loom` crate
- MIDI sequencer: verify sample-accurate event scheduling

### Integration Tests
- Full audio graph: load project, render offline, compare output WAV
- Plugin hosting: load test VST3, process audio, verify non-silence
- Round-trip: save project → load project → verify equality

### Benchmarks (Criterion)
- Audio graph processing: N tracks × M effects
- DSP primitives: filter, oscillator, FFT
- Waveform peak computation

```rust
// Example benchmark pattern
#[bench]
fn bench_oscillator_1024_frames(b: &mut Bencher) {
    let mut osc = WavetableOscillator::new(44100.0);
    let mut buffer = vec![0.0f32; 1024];
    b.iter(|| {
        osc.render(&mut buffer, 1024, 44100.0);
    });
}
```

---

## 📐 Code Style & Conventions

### Naming
- Crates: `ma-xxx` (kebab-case, "ma" = milolew Audio)
- Modules: `snake_case`
- Types: `PascalCase`
- Functions: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Audio buffer parameters: always `buffer: &mut [f32]`, `num_frames: u32`, `sample_rate: f32`

### Error Handling
- `thiserror` for library crate errors (ma-core, ma-audio-engine, etc.)
- `anyhow` only in ma-app (top-level binary)
- Audio thread: NEVER return `Result` — handle errors internally, output silence
- UI thread: show errors in status bar, never panic

### Documentation
- All public types and functions: `///` doc comments
- All modules: `//!` module-level docs explaining purpose
- Complex DSP: link to algorithm paper/resource in comments
- Unsafe blocks: ALWAYS explain WHY it's safe in comment

### Patterns to Follow
- **Builder pattern** for complex struct construction (TrackBuilder, SynthPatchBuilder)
- **Newtype pattern** for type-safe IDs: `pub struct TrackId(Uuid);`
- **Trait objects** for polymorphism (dyn Instrument, dyn PluginHost)
- **Interior mutability** (AtomicXxx) for audio-thread-shared state
- **Immutable data + atomic swap** for large state changes to audio thread

### Anti-Patterns to AVOID
- `unwrap()` in library code — always handle errors
- `clone()` in hot paths — profile first, clone is often a sign of bad ownership
- `Arc<Mutex<T>>` shared between audio and UI threads — use lock-free alternatives
- Raw pointers without clear safety justification
- Nested `Result<Result<T, E1>, E2>` — flatten error types

---

## 🚀 Development Phases

### Phase 0: Foundation (Weeks 1-4)
- [ ] Cargo workspace setup with all crates (empty scaffolds)
- [ ] CI/CD: GitHub Actions (test, clippy, fmt, bench)
- [ ] cpal audio output: play 440Hz sine wave on all platforms
- [ ] cpal audio input: record and save to WAV file
- [ ] ASIO feature flag verified on Windows
- [ ] Basic egui window with transport buttons (Play/Stop/Record)
- [ ] Lock-free SPSC ring buffer implementation + tests

### Phase 1: Audio Engine Core (Months 2-4)
- [ ] Audio file loading (WAV via hound, multi-format via symphonia)
- [ ] Audio buffer management and mixing
- [ ] Transport system (play, stop, pause, seek, loop)
- [ ] Multi-track audio playback with volume/pan
- [ ] Audio recording to track
- [ ] Basic arrangement timeline (egui custom widget)
- [ ] Waveform display with peak mipmaps

### Phase 2: MIDI & Synthesis (Months 4-6)
- [ ] MIDI input/output via midir
- [ ] MIDI event sequencer with tick-accurate scheduling
- [ ] Piano roll editor (egui custom widget)
- [ ] Wavetable synthesizer (sine/saw/square/triangle, 16 voices)
- [ ] ADSR envelope generator
- [ ] SVF filter with cutoff/resonance
- [ ] MIDI recording and quantization

### Phase 3: Mixer & Effects (Months 6-9)
- [ ] Mixer view with channel strips
- [ ] Built-in effects: EQ (parametric 4-band), Compressor, Reverb, Delay
- [ ] Effect chain per track
- [ ] Bus/Send routing
- [ ] Peak meters (lock-free, per-track + master)
- [ ] Parameter automation (envelope curves on timeline)

### Phase 4: VST3 Hosting (Months 9-12)
- [ ] VST3 plugin scanning (worker thread)
- [ ] VST3 plugin loading and instantiation
- [ ] VST3 audio processing integration
- [ ] VST3 GUI hosting (platform window embedding)
- [ ] VST3 parameter discovery and control
- [ ] VST3 state save/load (preset management)

### Phase 5: Polish & Community Launch (Months 12-18)
- [ ] Project save/load (JSON + audio file management)
- [ ] Undo/redo for all operations
- [ ] Keyboard shortcuts (Ableton-inspired defaults)
- [ ] Audio export/bounce (offline render)
- [ ] Time stretching (via rubato crate or custom)
- [ ] Subtractive synth + FM synth
- [ ] Session View / clip launcher
- [ ] CONTRIBUTING.md, architecture docs
- [ ] Community launch: GitHub, Discord, KVR, Reddit

---

## 🤖 Claude Code Workflow Tips

### Slash Commands (create these as custom commands)

- `/new-dsp` — Scaffold a new DSP processor in ma-dsp with trait impl, tests, benchmark
- `/new-synth` — Scaffold a new synthesizer in ma-synth with Instrument trait impl
- `/new-widget` — Scaffold a new egui widget in ma-ui with the standard pattern
- `/rt-check` — Review a file for real-time audio thread violations
- `/bench` — Generate a Criterion benchmark for a specific function

### Effective Prompting Patterns

```
# Architecture planning (do this BEFORE coding)
"Plan the architecture for [feature]. List all types, traits, and data flow.
Show the thread boundary. Do NOT write code yet."

# Implementation with RT safety
"Implement [feature] in ma-audio-engine. Remember: this runs on the audio
thread. No allocations, no locks, no I/O. Use the SPSC ring buffer for
communication with UI."

# DSP implementation
"Implement a [algorithm] based on [paper/reference]. Include:
1. The DSP code with #[inline] on hot functions
2. Unit test comparing output to known reference values
3. Criterion benchmark"

# Code review
"Review this code for: 1) real-time safety violations 2) potential panics
3) unnecessary allocations 4) thread safety issues"
```

### Git Workflow
- `main` — stable, all tests pass
- `dev` — integration branch
- `feature/xxx` — feature branches (short-lived)
- Commit messages: `feat(ma-audio-engine): add transport loop support`
- Use conventional commits: `feat`, `fix`, `refactor`, `test`, `docs`, `bench`

---

## 📚 Essential References

### Architecture & Real-Time Audio
- Ross Bencina: "Real-time Audio Programming 101" — rossbencina.com
- Timur Doumler: "Using Locks in Real-Time Audio Processing, Safely" — timur.audio
- The Audio Programmer podcast: "How DAWs Work" with Dave Rowland (Tracktion)

### Rust Audio Ecosystem
- RustAudio GitHub org: github.com/RustAudio
- cpal documentation: docs.rs/cpal
- Meadowlark post-mortem: billydm.github.io/blog/why-im-taking-a-break-from-meadowlark/

### DSP
- Julius O. Smith: "Mathematics of the DFT" & "Introduction to Digital Filters" — ccrma.stanford.edu/~jos/
- Andrew Simper (Cytomic): SVF filter design papers — cytomic.com/technical-papers
- Martin Finke: PolyBLEP oscillator implementation

### Open Source DAWs to Study
- Tracktion Engine: github.com/Tracktion/tracktion_engine (C++, architecture reference)
- Ardour: github.com/Ardour/ardour (C++, routing & session management)
- LMMS: github.com/LMMS/lmms (C++/Qt, built-in instruments)

---

## ⚠️ Known Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| egui not flexible enough for DAW UI | High | Custom epaint rendering, evaluate switching to iced if blocked |
| VST3 hosting in pure Rust is hard | High | Start with `vst3-sys`, accept some unsafe code with careful wrappers |
| ASIO on Windows requires SDK | Medium | Document setup clearly, provide WASAPI fallback |
| Sole developer burnout | High | Ship small, celebrate milestones, seek contributors early |
| Lock-free correctness | High | Use `loom` for testing, start with proven crates (ringbuf, crossbeam) |