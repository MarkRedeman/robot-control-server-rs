use std::collections::HashMap;

use rustypot::servo::feetech::sts3215::Sts3215Controller;

use super::calibration;
use super::client::{ArmState, Joint, JointState, RobotClient};

// Re-export calibration types so existing `use feetech::{load_calibration, ArmCalibration}` still works.
pub use super::calibration::{load_calibration, ArmCalibration};

pub struct FeetechRobotClient {
    #[allow(dead_code)]
    serial_id: String,
    port: String,
    controller: std::sync::Mutex<Sts3215Controller>,
    calibration: Option<ArmCalibration>,
}

impl FeetechRobotClient {
    pub fn new(
        serial_id: String,
        port: String,
        baudrate: u32,
        calibration: Option<ArmCalibration>,
    ) -> Result<Self, anyhow::Error> {
        let serial_port = match serialport::new(&port, baudrate)
            .timeout(std::time::Duration::from_millis(100))
            .open()
        {
            Ok(p) => p,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to open serial port: {}", e));
            }
        };

        let controller = Sts3215Controller::new()
            .with_protocol_v1()
            .with_serial_port(serial_port);

        Ok(Self {
            serial_id,
            port,
            controller: std::sync::Mutex::new(controller),
            calibration,
        })
    }

    fn motor_ids() -> Vec<u8> {
        Joint::ALL.iter().map(|j| j.motor_id()).collect()
    }

    fn read_arm_state_impl(&self) -> Result<ArmState, anyhow::Error> {
        let ids = Self::motor_ids();
        let mut controller = self
            .controller
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let raw_data = controller
            .sync_read_raw_present_position(&ids)
            .map_err(|e| anyhow::anyhow!("Failed to read servo data: {}", e))?;

        let timestamp = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;

        let joints: Vec<JointState> = Joint::ALL
            .iter()
            .enumerate()
            .map(|(i, &joint)| {
                let raw_position = raw_data[i];

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

    fn set_joints_impl(&self, positions: HashMap<String, f64>) -> Result<ArmState, anyhow::Error> {
        let ids = Self::motor_ids();
        let mut controller = self
            .controller
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut target_positions: Vec<f64> = Vec::with_capacity(6);
        for joint in Joint::ALL.iter() {
            if let Some(&target_deg) = positions.get(joint.name()) {
                let target_rad = target_deg * std::f64::consts::PI / 180.0;
                let raw =
                    (target_rad + std::f64::consts::PI) / (2.0 * std::f64::consts::PI) * 4096.0;
                target_positions.push(raw);
            } else {
                target_positions.push(0.0);
            }
        }

        controller
            .sync_write_goal_position(&ids, &target_positions)
            .map_err(|e| anyhow::anyhow!("Failed to write positions: {}", e))?;

        drop(controller);
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.read_arm_state_impl()
    }
}

impl RobotClient for FeetechRobotClient {
    fn port(&self) -> &str {
        &self.port
    }

    fn is_connected(&self) -> bool {
        self.controller.lock().map(|_| true).unwrap_or(false)
    }

    fn read_state(&self, _normalize: bool) -> Result<ArmState, anyhow::Error> {
        self.read_arm_state_impl()
    }

    fn set_joints_state(
        &self,
        positions: HashMap<String, f64>,
        _normalize: bool,
    ) -> Result<ArmState, anyhow::Error> {
        self.set_joints_impl(positions)
    }

    fn enable_torque(&self) -> Result<serde_json::Value, anyhow::Error> {
        let ids = Self::motor_ids();
        let mut controller = self
            .controller
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let enable = vec![true; 6];
        controller
            .sync_write_torque_enable(&ids, &enable)
            .map_err(|e| anyhow::anyhow!("Failed to enable torque: {}", e))?;

        Ok(serde_json::json!({
            "event": "torque_enabled",
            "success": true
        }))
    }

    fn disable_torque(&self) -> Result<serde_json::Value, anyhow::Error> {
        let ids = Self::motor_ids();
        let mut controller = self
            .controller
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let disable = vec![false; 6];
        controller
            .sync_write_torque_enable(&ids, &disable)
            .map_err(|e| anyhow::anyhow!("Failed to disable torque: {}", e))?;

        Ok(serde_json::json!({
            "event": "torque_disabled",
            "success": true
        }))
    }
}
