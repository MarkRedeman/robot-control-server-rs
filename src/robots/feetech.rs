use std::collections::HashMap;
use std::path::Path;

use rustypot::servo::feetech::sts3215::Sts3215Controller;

use super::calibration;
use super::client::{ArmState, Joint, JointState, RobotClient};
use super::driver::{RobotDriver, RobotObservation};
use super::units::JointUnit;

// Re-export calibration types so existing `use feetech::{load_calibration, ArmCalibration}` still works.
pub use super::calibration::{ArmCalibration, load_calibration};

/// Ordered joint names, matching [`Joint::ALL`].
const JOINT_NAMES: [&str; 6] = [
    "shoulder_pan",
    "shoulder_lift",
    "elbow_flex",
    "wrist_flex",
    "wrist_roll",
    "gripper",
];

/// STS3215 encoder resolution (12-bit, 0..=4095).
const MAX_RESOLUTION: i16 = 4095;

pub struct FeetechRobotClient {
    port: String,
    baudrate: u32,
    calibration: Option<ArmCalibration>,
    unit: JointUnit,
    controller: std::sync::Mutex<Option<Sts3215Controller>>,
}

impl FeetechRobotClient {
    /// Create a client for an SO-101 arm without opening the serial port.
    ///
    /// Call [`connect`](Self::connect) to establish the connection.
    ///
    /// `unit` must be [`JointUnit::Ticks`] when `calibration` is `None`, and
    /// [`JointUnit::Normalized`] or [`JointUnit::Degrees`] when it is `Some`.
    pub fn new(
        port: String,
        baudrate: u32,
        calibration: Option<ArmCalibration>,
        unit: JointUnit,
    ) -> anyhow::Result<Self> {
        match (calibration.is_some(), unit) {
            (true, JointUnit::Ticks) => {
                anyhow::bail!(
                    "Calibrated mode does not support unit='ticks'. Use 'normalized' or 'degrees'."
                )
            }
            (false, JointUnit::Normalized | JointUnit::Degrees) => {
                anyhow::bail!("Uncalibrated mode only supports unit='ticks'.")
            }
            _ => {}
        }

        Ok(Self {
            port,
            baudrate,
            calibration,
            unit,
            controller: std::sync::Mutex::new(None),
        })
    }

    pub fn joint_names(&self) -> &'static [&'static str] {
        &JOINT_NAMES
    }

    pub fn device_id(&self) -> String {
        device_id_from_serial_port(&self.port)
    }

    /// Establish the connection to the hardware. Idempotent.
    pub fn connect(&self) -> anyhow::Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        let serial_port = serialport::new(&self.port, self.baudrate)
            .timeout(std::time::Duration::from_millis(100))
            .open()
            .map_err(|e| anyhow::anyhow!("Failed to open serial port: {}", e))?;

        let controller = Sts3215Controller::new()
            .with_protocol_v1()
            .with_serial_port(serial_port);

        let mut guard = self
            .controller
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        *guard = Some(controller);
        Ok(())
    }

    /// Disconnect, leaving the robot in a safe state. Idempotent.
    pub fn disconnect(&self) -> anyhow::Result<()> {
        let mut guard = self
            .controller
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        if let Some(controller) = guard.as_mut() {
            // Best-effort torque off before dropping the controller (which
            // closes the port).
            let ids = Self::motor_ids();
            let disable = vec![false; ids.len()];
            let _ = controller.sync_write_torque_enable(&ids, &disable);
        }

        *guard = None;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.controller
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    pub fn get_observation(&self) -> anyhow::Result<RobotObservation> {
        let raw = self.read_raw_positions()?;
        Ok(RobotObservation {
            joint_positions: self.convert_raw_to_unit(&raw),
        })
    }

    pub fn send_action(&self, action: &[f64]) -> anyhow::Result<()> {
        let raw = self.convert_unit_to_raw(action)?;
        let ids = Self::motor_ids();

        self.with_controller(|controller| {
            controller
                .sync_write_raw_goal_position(&ids, &raw)
                .map_err(|e| anyhow::anyhow!("Failed to write positions: {}", e))
        })?;

        // Enable torque after writing the new goal (matches SO-101 follower behavior).
        self.set_torque(true)
    }

    pub fn set_torque(&self, enabled: bool) -> anyhow::Result<()> {
        let ids = Self::motor_ids();
        let values = vec![enabled; ids.len()];
        self.with_controller(|controller| {
            controller
                .sync_write_torque_enable(&ids, &values)
                .map_err(|e| anyhow::anyhow!("Failed to set torque: {}", e))
        })
    }

    fn motor_ids() -> Vec<u8> {
        Joint::ALL.iter().map(|j| j.motor_id()).collect()
    }

    /// Run `f` with the connected controller, returning a clear error when
    /// the robot has not been connected.
    fn with_controller<T>(
        &self,
        f: impl FnOnce(&mut Sts3215Controller) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut guard = self
            .controller
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let controller = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Robot is not connected. Call connect() first."))?;
        f(controller)
    }

    fn read_raw_positions(&self) -> anyhow::Result<Vec<i16>> {
        let ids = Self::motor_ids();
        self.with_controller(|controller| {
            controller
                .sync_read_raw_present_position(&ids)
                .map_err(|e| anyhow::anyhow!("Failed to read servo data: {}", e))
        })
    }

    /// Convert raw tick positions to the configured unit.
    fn convert_raw_to_unit(&self, raw: &[i16]) -> Vec<f64> {
        Joint::ALL
            .iter()
            .enumerate()
            .map(|(i, &joint)| self.convert_position(i32::from(raw[i]), joint))
            .collect()
    }

    fn convert_position(&self, raw_position: i32, joint: Joint) -> f64 {
        let Some(cal) = &self.calibration else {
            return f64::from(raw_position);
        };
        let Some(jc) = cal.get(joint.name()) else {
            return f64::from(raw_position);
        };
        let is_gripper = joint == Joint::Gripper;
        match self.unit {
            JointUnit::Ticks => f64::from(raw_position),
            JointUnit::Normalized => {
                if is_gripper {
                    calibration::calibrated_percentage(raw_position, jc)
                } else {
                    calibration::calibrated_m100_100(raw_position, jc)
                }
            }
            JointUnit::Degrees => {
                if is_gripper {
                    calibration::calibrated_percentage(raw_position, jc)
                } else {
                    calibration::calibrated_degrees(raw_position, jc)
                }
            }
        }
    }

    /// Convert unit values to raw tick positions.
    fn convert_unit_to_raw(&self, values: &[f64]) -> anyhow::Result<Vec<i16>> {
        if values.len() != Joint::ALL.len() {
            anyhow::bail!(
                "Expected {} joint values, got {}",
                Joint::ALL.len(),
                values.len()
            );
        }
        Ok(Joint::ALL
            .iter()
            .enumerate()
            .map(|(i, &joint)| self.convert_value(values[i], joint))
            .collect())
    }

    fn convert_value(&self, value: f64, joint: Joint) -> i16 {
        let Some(cal) = &self.calibration else {
            return value.clamp(0.0, f64::from(MAX_RESOLUTION)).round() as i16;
        };
        let Some(jc) = cal.get(joint.name()) else {
            return value.clamp(0.0, f64::from(MAX_RESOLUTION)).round() as i16;
        };
        let is_gripper = joint == Joint::Gripper;
        let raw = match self.unit {
            JointUnit::Ticks => value.clamp(0.0, f64::from(MAX_RESOLUTION)).round() as i16,
            JointUnit::Normalized => {
                if is_gripper {
                    calibration::unnormalize_percentage(value, jc)
                } else {
                    calibration::unnormalize_m100_100(value, jc)
                }
            }
            JointUnit::Degrees => {
                if is_gripper {
                    calibration::unnormalize_percentage(value, jc)
                } else {
                    calibration::unnormalize_degrees(value, jc)
                }
            }
        };
        raw.clamp(0, MAX_RESOLUTION)
    }

    fn read_arm_state_impl(&self) -> anyhow::Result<ArmState> {
        let raw_positions = self.read_raw_positions()?;
        let timestamp = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;

        let joints: Vec<JointState> = Joint::ALL
            .iter()
            .enumerate()
            .map(|(i, &joint)| {
                let raw_position = raw_positions[i];

                // The servo returns i16 via two's-complement (from_le_bytes).
                // For typical positions (0..4095) this matches lerobot's
                // sign-magnitude decode. Widen to i32 for the calibration math.
                let decoded_position = i32::from(raw_position);

                let calibrated_angle = self
                    .calibration
                    .as_ref()
                    .and_then(|cal| cal.get(joint.name()))
                    .map(|jc| {
                        if joint == Joint::Gripper {
                            // Gripper uses RANGE_0_100 in lerobot.
                            calibration::calibrated_percentage(decoded_position, jc)
                        } else {
                            // Non-gripper joints use RANGE_M100_100 by default in lerobot
                            // (when use_degrees=false, which is the default).
                            calibration::calibrated_m100_100(decoded_position, jc)
                        }
                    });

                JointState {
                    joint: joint.name().to_string(),
                    motor_id: joint.motor_id(),
                    raw_position,
                    calibrated_angle,
                }
            })
            .collect();

        Ok(ArmState { timestamp, joints })
    }

    fn set_joints_impl(&self, positions: HashMap<String, f64>) -> anyhow::Result<ArmState> {
        let cal = self
            .calibration
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Cannot set joints without calibration data"))?;

        // Only command joints that are present in the input map.
        let mut ids: Vec<u8> = Vec::new();
        let mut raw_positions: Vec<i16> = Vec::new();

        for &joint in Joint::ALL.iter() {
            if let Some(&calibrated_value) = positions.get(joint.name()) {
                let jc = cal.get(joint.name()).ok_or_else(|| {
                    anyhow::anyhow!("No calibration data for joint '{}'", joint.name())
                })?;

                let raw = if joint == Joint::Gripper {
                    calibration::unnormalize_percentage(calibrated_value, jc)
                } else {
                    calibration::unnormalize_m100_100(calibrated_value, jc)
                };

                ids.push(joint.motor_id());
                raw_positions.push(raw);
            }
        }

        if ids.is_empty() {
            return self.read_arm_state_impl();
        }

        self.with_controller(|controller| {
            controller
                .sync_write_raw_goal_position(&ids, &raw_positions)
                .map_err(|e| anyhow::anyhow!("Failed to write positions: {}", e))
        })?;

        std::thread::sleep(std::time::Duration::from_millis(50));
        self.read_arm_state_impl()
    }
}

fn device_id_from_serial_port(port: &str) -> String {
    let name = if port.starts_with("/dev/") {
        std::fs::canonicalize(port)
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| file_name(port))
    } else {
        file_name(port)
    };
    format!("serial:{name}")
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

impl RobotDriver for FeetechRobotClient {
    fn joint_names(&self) -> &'static [&'static str] {
        self.joint_names()
    }

    fn device_id(&self) -> String {
        self.device_id()
    }

    fn connect(&self) -> anyhow::Result<()> {
        self.connect()
    }

    fn disconnect(&self) -> anyhow::Result<()> {
        self.disconnect()
    }

    fn is_connected(&self) -> bool {
        self.is_connected()
    }

    fn get_observation(&self) -> anyhow::Result<RobotObservation> {
        self.get_observation()
    }

    fn send_action(&self, action: &[f64]) -> anyhow::Result<()> {
        self.send_action(action)
    }

    fn set_torque(&self, enabled: bool) -> anyhow::Result<()> {
        self.set_torque(enabled)
    }
}

impl RobotClient for FeetechRobotClient {
    fn port(&self) -> &str {
        &self.port
    }

    fn is_connected(&self) -> bool {
        self.is_connected()
    }

    fn read_state(&self) -> Result<ArmState, anyhow::Error> {
        self.read_arm_state_impl()
    }

    fn set_joints_state(&self, positions: HashMap<String, f64>) -> Result<ArmState, anyhow::Error> {
        self.set_joints_impl(positions)
    }

    fn enable_torque(&self) -> Result<serde_json::Value, anyhow::Error> {
        self.set_torque(true)?;
        Ok(serde_json::json!({
            "event": "torque_enabled",
            "success": true
        }))
    }

    fn disable_torque(&self) -> Result<serde_json::Value, anyhow::Error> {
        self.set_torque(false)?;
        Ok(serde_json::json!({
            "event": "torque_disabled",
            "success": true
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::calibration::JointCalibration;
    use super::*;

    fn sample_calibration() -> ArmCalibration {
        let mut cal = HashMap::new();
        let entries = [
            ("shoulder_pan", 1, 0, 344, 694, 3424),
            ("shoulder_lift", 2, 0, -1254, 825, 3216),
            ("elbow_flex", 3, 0, 974, 899, 3127),
            ("wrist_flex", 4, 0, 798, 856, 3205),
            ("wrist_roll", 5, 0, -1103, 104, 3925),
            ("gripper", 6, 0, 1328, 1727, 2987),
        ];
        for (name, id, drive_mode, homing_offset, range_min, range_max) in entries {
            cal.insert(
                name.to_string(),
                JointCalibration {
                    id,
                    drive_mode,
                    homing_offset,
                    range_min,
                    range_max,
                },
            );
        }
        cal
    }

    fn calibrated_client(unit: JointUnit) -> FeetechRobotClient {
        FeetechRobotClient::new(
            "/dev/ttyUSB0".into(),
            1_000_000,
            Some(sample_calibration()),
            unit,
        )
        .unwrap()
    }

    fn uncalibrated_client() -> FeetechRobotClient {
        FeetechRobotClient::new("/dev/ttyUSB0".into(), 1_000_000, None, JointUnit::Ticks).unwrap()
    }

    /// Known lerobot values from readme.org: raw positions and the
    /// corresponding normalized values.
    #[test]
    fn normalized_positions_match_lerobot() {
        let client = calibrated_client(JointUnit::Normalized);
        let raw = [2023i32, 852, 3111, 2875, 2613, 2208];
        let expected = [
            -2.637362637362642,
            -97.74153074027603,
            98.5637342908438,
            71.9029374201788,
            31.32687778068569,
            38.17460317460317,
        ];
        for (i, &joint) in Joint::ALL.iter().enumerate() {
            let value = client.convert_position(raw[i], joint);
            assert!(
                (value - expected[i]).abs() < 1e-9,
                "{}: got {value}, expected {}",
                joint.name(),
                expected[i]
            );
        }
    }

    #[test]
    fn degrees_are_relative_to_range_midpoint() {
        let client = calibrated_client(JointUnit::Degrees);
        // shoulder_pan mid = (694 + 3424) / 2 = 2059.
        let value = client.convert_position(2023, Joint::Base);
        let expected = (2023.0 - 2059.0) * 360.0 / 4095.0;
        assert!((value - expected).abs() < 1e-9);

        // Gripper stays percentage even in degrees mode.
        let gripper = client.convert_position(2208, Joint::Gripper);
        assert!((gripper - 38.17460317460317).abs() < 1e-9);
    }

    #[test]
    fn ticks_are_passthrough() {
        let client = uncalibrated_client();
        assert_eq!(client.convert_position(2208, Joint::Base), 2208.0);
    }

    #[test]
    fn convert_round_trips_within_one_tick() {
        for unit in [JointUnit::Normalized, JointUnit::Degrees] {
            let client = calibrated_client(unit);
            let cal = client.calibration.as_ref().unwrap();
            for &joint in Joint::ALL.iter() {
                let jc = &cal[joint.name()];
                // Normalized mode clamps to the calibrated range, so test
                // within-range values (degrees does not clamp).
                let raws = [
                    jc.range_min,
                    (jc.range_min + jc.range_max) / 2,
                    jc.range_max,
                ];
                for &raw in &raws {
                    let value = client.convert_position(raw, joint);
                    let back = client.convert_value(value, joint);
                    assert!(
                        (i32::from(back) - raw).abs() <= 1,
                        "unit={:?} joint={} raw={raw}: value={value} back={back}",
                        unit,
                        joint.name()
                    );
                }
            }
        }
    }

    #[test]
    fn unit_calibration_combinations_are_validated() {
        assert!(
            FeetechRobotClient::new(
                "/dev/ttyUSB0".into(),
                1_000_000,
                Some(sample_calibration()),
                JointUnit::Ticks,
            )
            .is_err()
        );
        assert!(
            FeetechRobotClient::new(
                "/dev/ttyUSB0".into(),
                1_000_000,
                None,
                JointUnit::Normalized,
            )
            .is_err()
        );
        assert!(
            FeetechRobotClient::new("/dev/ttyUSB0".into(), 1_000_000, None, JointUnit::Ticks,)
                .is_ok()
        );
    }
}
