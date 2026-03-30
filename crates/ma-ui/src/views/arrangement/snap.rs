//! Snap grid for arrangement view — broader set than piano roll QuantizeGrid.

use crate::types::time::{Tick, PPQN};

/// Snap grid resolution for the arrangement view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapGrid {
    Off,
    Bar,
    Half,
    #[default]
    Quarter,
    Eighth,
    Sixteenth,
}

impl SnapGrid {
    /// Grid resolution in ticks. Bar depends on time signature numerator.
    pub fn ticks(self, beats_per_bar: u8) -> Tick {
        match self {
            Self::Off => 1,
            Self::Bar => PPQN * beats_per_bar as i64,
            Self::Half => PPQN * 2,
            Self::Quarter => PPQN,
            Self::Eighth => PPQN / 2,
            Self::Sixteenth => PPQN / 4,
        }
    }

    /// Snap to nearest grid line.
    pub fn snap(self, tick: Tick, beats_per_bar: u8) -> Tick {
        let grid = self.ticks(beats_per_bar);
        (tick + grid / 2).div_euclid(grid) * grid
    }

    /// Snap down (floor) to grid line.
    pub fn snap_floor(self, tick: Tick, beats_per_bar: u8) -> Tick {
        let grid = self.ticks(beats_per_bar);
        tick.div_euclid(grid) * grid
    }

    /// Display name for UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Bar => "Bar",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_at_4_4() {
        assert_eq!(SnapGrid::Bar.ticks(4), 3840);
        assert_eq!(SnapGrid::Half.ticks(4), 1920);
        assert_eq!(SnapGrid::Quarter.ticks(4), 960);
        assert_eq!(SnapGrid::Eighth.ticks(4), 480);
        assert_eq!(SnapGrid::Sixteenth.ticks(4), 240);
        assert_eq!(SnapGrid::Off.ticks(4), 1);
    }

    #[test]
    fn ticks_at_3_4() {
        assert_eq!(SnapGrid::Bar.ticks(3), 2880);
    }

    #[test]
    fn snap_nearest() {
        assert_eq!(SnapGrid::Quarter.snap(100, 4), 0);
        assert_eq!(SnapGrid::Quarter.snap(500, 4), 960);
        assert_eq!(SnapGrid::Quarter.snap(480, 4), 960);
        assert_eq!(SnapGrid::Eighth.snap(300, 4), 480);
    }

    #[test]
    fn snap_floor_rounds_down() {
        assert_eq!(SnapGrid::Quarter.snap_floor(100, 4), 0);
        assert_eq!(SnapGrid::Quarter.snap_floor(959, 4), 0);
        assert_eq!(SnapGrid::Quarter.snap_floor(960, 4), 960);
    }

    #[test]
    fn snap_negative_ticks() {
        assert_eq!(SnapGrid::Quarter.snap_floor(-100, 4), -960);
        assert_eq!(SnapGrid::Quarter.snap_floor(-960, 4), -960);
        assert_eq!(SnapGrid::Eighth.snap(-100, 4), 0);
        assert_eq!(SnapGrid::Eighth.snap(-300, 4), -480);
    }

    #[test]
    fn snap_off_is_identity() {
        assert_eq!(SnapGrid::Off.snap(123, 4), 123);
        assert_eq!(SnapGrid::Off.snap_floor(123, 4), 123);
    }

    #[test]
    fn snap_bar_3_4() {
        // 3/4 bar = 2880 ticks
        assert_eq!(SnapGrid::Bar.snap(1000, 3), 0);
        assert_eq!(SnapGrid::Bar.snap(1500, 3), 2880);
        assert_eq!(SnapGrid::Bar.snap_floor(2879, 3), 0);
        assert_eq!(SnapGrid::Bar.snap_floor(2880, 3), 2880);
    }
}
