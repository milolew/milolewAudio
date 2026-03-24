//! Browser state — file/sample browser for loading audio and MIDI files.
//!
//! Tracks the current directory, file listing, selected item, and filter mode.
//! Files are loaded lazily when the user navigates directories.

use std::path::PathBuf;

/// Supported file type filters for the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserFilter {
    /// Show all supported files.
    #[default]
    All,
    /// Show only audio files (WAV, FLAC, OGG, MP3).
    Audio,
    /// Show only MIDI files (.mid, .midi).
    Midi,
}

/// A single entry in the browser file list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserEntry {
    /// Display name (file or directory name, no path).
    pub name: String,
    /// Full path to the file or directory.
    pub path: PathBuf,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// File extension (lowercase, empty for directories).
    pub extension: String,
}

impl BrowserEntry {
    /// Whether this entry is an audio file.
    pub fn is_audio(&self) -> bool {
        matches!(
            self.extension.as_str(),
            "wav" | "flac" | "ogg" | "mp3" | "aiff" | "aif"
        )
    }

    /// Whether this entry is a MIDI file.
    pub fn is_midi(&self) -> bool {
        matches!(self.extension.as_str(), "mid" | "midi")
    }
}

/// Browser panel state.
#[derive(Debug, Clone)]
pub struct BrowserState {
    /// Current directory being browsed.
    pub current_dir: PathBuf,
    /// Entries in the current directory (files + subdirectories).
    pub entries: Vec<BrowserEntry>,
    /// Index of the selected entry (None if nothing selected).
    pub selected_index: Option<usize>,
    /// Active file type filter.
    pub filter: BrowserFilter,
    /// Whether the browser panel is visible.
    pub visible: bool,
}

impl Default for BrowserState {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            current_dir: home,
            entries: Vec::new(),
            selected_index: None,
            filter: BrowserFilter::default(),
            visible: false,
        }
    }
}

impl BrowserState {
    /// Refresh the file listing for the current directory.
    ///
    /// Reads the filesystem and populates `entries` with directories first,
    /// then files matching the current filter, both sorted alphabetically.
    pub fn refresh(&mut self) {
        self.entries.clear();
        self.selected_index = None;

        let read_dir = match std::fs::read_dir(&self.current_dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files/dirs
            if name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                dirs.push(BrowserEntry {
                    name,
                    path,
                    is_dir: true,
                    extension: String::new(),
                });
            } else {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();

                let entry = BrowserEntry {
                    name,
                    path,
                    is_dir: false,
                    extension: ext,
                };

                // Apply filter
                let passes = match self.filter {
                    BrowserFilter::All => entry.is_audio() || entry.is_midi(),
                    BrowserFilter::Audio => entry.is_audio(),
                    BrowserFilter::Midi => entry.is_midi(),
                };

                if passes {
                    files.push(entry);
                }
            }
        }

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        self.entries.extend(dirs);
        self.entries.extend(files);
    }

    /// Navigate into a subdirectory.
    pub fn enter_dir(&mut self, path: PathBuf) {
        self.current_dir = path;
        self.refresh();
    }

    /// Navigate to the parent directory.
    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.refresh();
        }
    }

    /// Get the currently selected entry, if any.
    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.selected_index.and_then(|i| self.entries.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn browser_entry_audio_detection() {
        let wav = BrowserEntry {
            name: "kick.wav".into(),
            path: PathBuf::from("/samples/kick.wav"),
            is_dir: false,
            extension: "wav".into(),
        };
        assert!(wav.is_audio());
        assert!(!wav.is_midi());

        let flac = BrowserEntry {
            name: "pad.flac".into(),
            path: PathBuf::from("/samples/pad.flac"),
            is_dir: false,
            extension: "flac".into(),
        };
        assert!(flac.is_audio());
    }

    #[test]
    fn browser_entry_midi_detection() {
        let mid = BrowserEntry {
            name: "melody.mid".into(),
            path: PathBuf::from("/projects/melody.mid"),
            is_dir: false,
            extension: "mid".into(),
        };
        assert!(mid.is_midi());
        assert!(!mid.is_audio());

        let midi = BrowserEntry {
            name: "chords.midi".into(),
            path: PathBuf::from("/projects/chords.midi"),
            is_dir: false,
            extension: "midi".into(),
        };
        assert!(midi.is_midi());
    }

    #[test]
    fn browser_entry_directory_is_not_audio_or_midi() {
        let dir = BrowserEntry {
            name: "Samples".into(),
            path: PathBuf::from("/home/Samples"),
            is_dir: true,
            extension: String::new(),
        };
        assert!(!dir.is_audio());
        assert!(!dir.is_midi());
    }

    #[test]
    fn default_browser_state() {
        let state = BrowserState::default();
        assert!(!state.visible);
        assert_eq!(state.filter, BrowserFilter::default());
        assert!(state.entries.is_empty());
        assert!(state.selected_index.is_none());
    }

    #[test]
    fn selected_entry_none_when_empty() {
        let state = BrowserState::default();
        assert!(state.selected_entry().is_none());
    }

    #[test]
    fn selected_entry_out_of_bounds() {
        let mut state = BrowserState::default();
        state.selected_index = Some(999);
        assert!(state.selected_entry().is_none());
    }

    #[test]
    fn filter_default_is_all() {
        assert_eq!(BrowserFilter::default(), BrowserFilter::All);
    }

    #[test]
    fn refresh_populates_entries_from_tempdir() {
        let temp = std::env::temp_dir().join("ma_browser_test");
        let _ = std::fs::create_dir_all(&temp);

        // Create test files
        let wav_path = temp.join("test.wav");
        let mid_path = temp.join("test.mid");
        let txt_path = temp.join("readme.txt");
        let subdir_path = temp.join("subdir");

        let _ = std::fs::write(&wav_path, b"fake wav");
        let _ = std::fs::write(&mid_path, b"fake midi");
        let _ = std::fs::write(&txt_path, b"text file");
        let _ = std::fs::create_dir_all(&subdir_path);

        let mut state = BrowserState {
            current_dir: temp.clone(),
            ..Default::default()
        };
        state.refresh();

        // Should have subdir + wav + mid (txt filtered out by All filter)
        let dir_count = state.entries.iter().filter(|e| e.is_dir).count();
        let file_count = state.entries.iter().filter(|e| !e.is_dir).count();
        assert!(dir_count >= 1, "Expected at least 1 directory");
        assert!(file_count >= 2, "Expected at least 2 files (wav + mid)");

        // Directories should come first
        if let Some(first) = state.entries.first() {
            assert!(first.is_dir);
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn audio_filter_excludes_midi() {
        let temp = std::env::temp_dir().join("ma_browser_filter_test");
        let _ = std::fs::create_dir_all(&temp);

        let _ = std::fs::write(temp.join("kick.wav"), b"wav");
        let _ = std::fs::write(temp.join("melody.mid"), b"mid");

        let mut state = BrowserState {
            current_dir: temp.clone(),
            filter: BrowserFilter::Audio,
            ..Default::default()
        };
        state.refresh();

        let midi_files = state.entries.iter().filter(|e| e.is_midi()).count();
        assert_eq!(midi_files, 0, "Audio filter should exclude MIDI files");

        let audio_files = state.entries.iter().filter(|e| e.is_audio()).count();
        assert!(audio_files >= 1, "Audio filter should include WAV files");

        let _ = std::fs::remove_dir_all(&temp);
    }
}
