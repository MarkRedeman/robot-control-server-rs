//! Joint-space units for robot observations and actions.

use std::str::FromStr;

/// Unit in which joint positions are expressed.
///
/// Mirrors the SO-101 driver convention:
///
/// * [`JointUnit::Normalized`] — body joints in `[-100, 100]`, gripper in
///   `[0, 100]` (requires calibration).
/// * [`JointUnit::Degrees`] — body joints in degrees relative to each
///   calibrated range midpoint, gripper still in `[0, 100]` (requires
///   calibration).
/// * [`JointUnit::Ticks`] — raw servo encoder ticks `[0, 4095]` (uncalibrated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointUnit {
    Normalized,
    Degrees,
    Ticks,
}

impl JointUnit {
    /// Lower-case string form used across the Python boundary.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normalized => "normalized",
            Self::Degrees => "degrees",
            Self::Ticks => "ticks",
        }
    }
}

impl FromStr for JointUnit {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "normalized" => Ok(Self::Normalized),
            "degrees" => Ok(Self::Degrees),
            "ticks" => Ok(Self::Ticks),
            other => Err(format!(
                "Invalid unit {other:?}. Must be one of \"normalized\", \"degrees\", \"ticks\"."
            )),
        }
    }
}
