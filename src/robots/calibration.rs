use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Per-joint calibration data from a lerobot calibration JSON file.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[allow(dead_code)]
pub fn by_motor_id(cal: &ArmCalibration) -> HashMap<u8, &JointCalibration> {
    cal.values().map(|jc| (jc.id, jc)).collect()
}

/// Apply lerobot DEGREES-mode calibration.
///
/// The servo firmware has already applied the homing_offset, so the value
/// read from the bus is the "homed" position.
///
///   mid = (range_min + range_max) / 2
///   degrees = (position - mid) * 360 / 4095
///
/// Note: DEGREES mode does NOT clamp and does NOT apply drive_mode
/// (matches lerobot's `_normalize` for `MotorNormMode.DEGREES`).
#[allow(dead_code)]
pub fn calibrated_degrees(position: i32, jc: &JointCalibration) -> f64 {
    let mid = (f64::from(jc.range_min) + f64::from(jc.range_max)) / 2.0;
    (f64::from(position) - mid) * 360.0 / MAX_RESOLUTION
}

/// Apply lerobot RANGE_M100_100 calibration (default for SO-101 non-gripper joints).
///
/// When `use_degrees` is false (the lerobot default), non-gripper joints
/// use this mode instead of DEGREES.
///
///   clamped = clamp(position, range_min, range_max)
///   norm = ((clamped - min) / (max - min)) * 200 - 100
///   result = -norm if drive_mode else norm
pub fn calibrated_m100_100(position: i32, jc: &JointCalibration) -> f64 {
    let min = f64::from(jc.range_min);
    let max = f64::from(jc.range_max);
    let clamped = f64::from(position).clamp(min, max);
    let norm = ((clamped - min) / (max - min)) * 200.0 - 100.0;
    if jc.drive_mode != 0 { -norm } else { norm }
}

/// Apply lerobot RANGE_0_100 calibration (used for the gripper).
///
///   clamped = clamp(position, range_min, range_max)
///   pct = ((clamped - min) / (max - min)) * 100
///   result = (100 - pct) if drive_mode else pct
pub fn calibrated_percentage(position: i32, jc: &JointCalibration) -> f64 {
    let min = f64::from(jc.range_min);
    let max = f64::from(jc.range_max);
    let clamped = f64::from(position).clamp(min, max);
    let pct = (clamped - min) / (max - min) * 100.0;
    if jc.drive_mode != 0 { 100.0 - pct } else { pct }
}

// ---------------------------------------------------------------------------
// Unnormalize (inverse) functions: calibrated value → raw servo position
// ---------------------------------------------------------------------------

/// Inverse of `calibrated_degrees`.
///
/// Matches lerobot's `_unnormalize` for `MotorNormMode.DEGREES`:
///   mid = (range_min + range_max) / 2
///   raw = (value * 4095 / 360) + mid
#[allow(dead_code)]
pub fn unnormalize_degrees(value: f64, jc: &JointCalibration) -> i16 {
    let mid = (f64::from(jc.range_min) + f64::from(jc.range_max)) / 2.0;
    let raw = (value * MAX_RESOLUTION / 360.0) + mid;
    raw as i16
}

/// Inverse of `calibrated_m100_100`.
///
/// Matches lerobot's `_unnormalize` for `MotorNormMode.RANGE_M100_100`:
///   val = -value if drive_mode else value
///   bounded = clamp(val, -100, 100)
///   raw = int(((bounded + 100) / 200) * (max - min) + min)
pub fn unnormalize_m100_100(value: f64, jc: &JointCalibration) -> i16 {
    let val = if jc.drive_mode != 0 { -value } else { value };
    let bounded = val.clamp(-100.0, 100.0);
    let min = f64::from(jc.range_min);
    let max = f64::from(jc.range_max);
    let raw = ((bounded + 100.0) / 200.0) * (max - min) + min;
    raw as i16
}

/// Inverse of `calibrated_percentage`.
///
/// Matches lerobot's `_unnormalize` for `MotorNormMode.RANGE_0_100`:
///   val = (100 - value) if drive_mode else value
///   bounded = clamp(val, 0, 100)
///   raw = int((bounded / 100) * (max - min) + min)
pub fn unnormalize_percentage(value: f64, jc: &JointCalibration) -> i16 {
    let val = if jc.drive_mode != 0 {
        100.0 - value
    } else {
        value
    };
    let bounded = val.clamp(0.0, 100.0);
    let min = f64::from(jc.range_min);
    let max = f64::from(jc.range_max);
    let raw = (bounded / 100.0) * (max - min) + min;
    raw as i16
}
