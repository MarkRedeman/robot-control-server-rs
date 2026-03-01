use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Per-joint calibration data from a lerobot calibration JSON file.
#[derive(Debug, Clone, Deserialize)]
pub struct JointCalibration {
    pub id: u8,
    pub drive_mode: u8,
    /// The homing offset is applied by the servo firmware internally.
    /// We store it for completeness (matches the lerobot JSON schema)
    /// but do not use it in the calibration math — present_position
    /// already has it baked in.
    #[allow(dead_code)]
    pub homing_offset: i32,
    pub range_min: i32,
    pub range_max: i32,
}

/// Full arm calibration, keyed by joint name.
pub type ArmCalibration = HashMap<String, JointCalibration>;

/// Resolution of the STS3215 encoder (12-bit, 0-4095).
const MAX_RESOLUTION: f64 = 4095.0;

/// Load calibration data from a lerobot-format JSON file.
pub fn load_calibration(path: &Path) -> Result<ArmCalibration> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read calibration file '{}'", path.display()))?;
    let cal: ArmCalibration = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse calibration file '{}'", path.display()))?;
    Ok(cal)
}

/// Build a lookup table from motor ID to JointCalibration.
pub fn by_motor_id(cal: &ArmCalibration) -> HashMap<u8, &JointCalibration> {
    cal.values().map(|jc| (jc.id, jc)).collect()
}

/// Apply lerobot DEGREES-mode calibration to a decoded present_position value.
///
/// The servo firmware has already applied the homing_offset, so the value
/// read from the bus is the "homed" position. The formula (matching lerobot's
/// `MotorNormMode.DEGREES`) is:
///
///   mid = (range_min + range_max) / 2
///   degrees = (position - mid) * 360 / 4095
///
/// For the gripper joint (id 6), lerobot uses `RANGE_0_100` mode:
///
///   clamped = clamp(position, range_min, range_max)
///   pct = (clamped - range_min) / (range_max - range_min) * 100
///   result = (100 - pct) if drive_mode else pct
///
/// `drive_mode` is ignored for DEGREES mode (all SO-101 arms use drive_mode=0
/// anyway).
pub fn calibrated_degrees(position: i32, jc: &JointCalibration) -> f64 {
    let mid = (jc.range_min as f64 + jc.range_max as f64) / 2.0;
    (position as f64 - mid) * 360.0 / MAX_RESOLUTION
}

/// Apply lerobot RANGE_0_100 calibration (used for the gripper).
pub fn calibrated_percentage(position: i32, jc: &JointCalibration) -> f64 {
    let min = jc.range_min as f64;
    let max = jc.range_max as f64;
    let clamped = (position as f64).clamp(min, max);
    let pct = (clamped - min) / (max - min) * 100.0;
    if jc.drive_mode != 0 {
        100.0 - pct
    } else {
        pct
    }
}
