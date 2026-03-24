//! Time representation types for the DAW.
//!
//! Musical time uses ticks (960 PPQN), absolute time uses samples.
//! The Transport is the only place that converts between them.

use serde::{Deserialize, Serialize};

/// Musical time unit — pulses per quarter note.
pub type Tick = i64;

/// Absolute sample position at project sample rate.
pub type SamplePos = i64;

/// Buffer frame count per audio callback.
pub type FrameCount = u32;

/// Pulses Per Quarter Note — standard resolution.
pub const PPQN: Tick = 960;

/// Time signature (e.g., 4/4, 3/4, 6/8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

/// Human-readable bar:beat:tick representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarBeatTick {
    pub bar: u32,
    pub beat: u8,
    pub tick: u16,
}

impl std::fmt::Display for BarBeatTick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:02}:{:03}", self.bar, self.beat, self.tick)
    }
}

/// A range of ticks [start, end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRange {
    pub start: Tick,
    pub end: Tick,
}

impl TickRange {
    pub fn new(start: Tick, end: Tick) -> Self {
        Self { start, end }
    }

    pub fn duration(&self) -> Tick {
        self.end - self.start
    }

    pub fn contains(&self, tick: Tick) -> bool {
        tick >= self.start && tick < self.end
    }
}

/// Quantization grid resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizeGrid {
    Off,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl QuantizeGrid {
    /// Grid resolution in ticks.
    pub fn ticks(self) -> Tick {
        match self {
            Self::Off => 1,
            Self::Quarter => PPQN,
            Self::Eighth => PPQN / 2,
            Self::Sixteenth => PPQN / 4,
            Self::ThirtySecond => PPQN / 8,
        }
    }

    /// Snap a tick value to the nearest grid line.
    /// Uses Euclidean division for correct behavior with negative ticks.
    pub fn snap(self, tick: Tick) -> Tick {
        let grid = self.ticks();
        (tick + grid / 2).div_euclid(grid) * grid
    }

    /// Snap a tick value down to the grid line at or before it.
    /// Uses Euclidean division for correct behavior with negative ticks.
    pub fn snap_floor(self, tick: Tick) -> Tick {
        let grid = self.ticks();
        tick.div_euclid(grid) * grid
    }
}

/// Convert ticks to bar:beat:tick display.
/// Clamps negative ticks to 0 to avoid nonsensical bar/beat values.
pub fn tick_to_bbt(tick: Tick, time_sig: TimeSignature) -> BarBeatTick {
    let tick = tick.max(0);
    let ticks_per_beat = PPQN;
    let ticks_per_bar = ticks_per_beat * time_sig.numerator as i64;

    let bar = (tick / ticks_per_bar) as u32 + 1;
    let remaining = tick % ticks_per_bar;
    let beat = (remaining / ticks_per_beat) as u8 + 1;
    let sub_tick = (remaining % ticks_per_beat) as u16;

    BarBeatTick {
        bar,
        beat,
        tick: sub_tick,
    }
}

/// Convert bar:beat:tick to absolute ticks.
pub fn bbt_to_tick(bbt: BarBeatTick, time_sig: TimeSignature) -> Tick {
    let ticks_per_beat = PPQN;
    let ticks_per_bar = ticks_per_beat * time_sig.numerator as i64;

    (bbt.bar as i64 - 1) * ticks_per_bar + (bbt.beat as i64 - 1) * ticks_per_beat + bbt.tick as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bbt_roundtrip() {
        let ts = TimeSignature::default();
        for tick in [0, 960, 1920, 3840, 4800] {
            let bbt = tick_to_bbt(tick, ts);
            assert_eq!(bbt_to_tick(bbt, ts), tick);
        }
    }

    #[test]
    fn test_snap_to_grid() {
        assert_eq!(QuantizeGrid::Quarter.snap(100), 0);
        assert_eq!(QuantizeGrid::Quarter.snap(500), 960);
        assert_eq!(QuantizeGrid::Eighth.snap(300), 480);
    }

    #[test]
    fn test_snap_negative_ticks() {
        assert_eq!(QuantizeGrid::Quarter.snap_floor(-100), -960);
        assert_eq!(QuantizeGrid::Quarter.snap_floor(-960), -960);
        assert_eq!(QuantizeGrid::Quarter.snap_floor(0), 0);
        assert_eq!(QuantizeGrid::Eighth.snap(-100), 0);
        assert_eq!(QuantizeGrid::Eighth.snap(-300), -480);
    }

    #[test]
    fn test_tick_to_bbt_clamps_negative() {
        let ts = TimeSignature::default();
        let bbt = tick_to_bbt(-100, ts);
        assert_eq!(bbt.bar, 1);
        assert_eq!(bbt.beat, 1);
        assert_eq!(bbt.tick, 0);
    }

    #[test]
    fn test_tick_range() {
        let range = TickRange::new(100, 500);
        assert!(range.contains(200));
        assert!(!range.contains(500));
        assert_eq!(range.duration(), 400);
    }
}
