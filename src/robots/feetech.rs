use std::collections::HashMap;

use rustypot::servo::feetech::sts3215::Sts3215Controller;
use serde::{Deserialize, Serialize};

use super::client::{ArmState, Joint, JointState, RobotClient};

const MAX_RESOLUTION: f64 = 4095.0;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JointCalibration {
    pub id: u8,
    pub drive_mode: u8,
    pub homing_offset: i32,
    pub range_min: i32,
    pub range_max: i32,
}

pub type ArmCalibration = HashMap<String, JointCalibration>;

pub fn load_calibration(path: &std::path::Path) -> Result<ArmCalibration, anyhow::Error> {
    let contents = std::fs::read_to_string(path)?;
    let cal: ArmCalibration = serde_json::from_str(&contents)?;
    Ok(cal)
}

fn calibrated_degrees(position: i32, jc: &JointCalibration) -> f64 {
    let mid = (f64::from(jc.range_min) + f64::from(jc.range_max)) / 2.0;
    (f64::from(position) - mid) * 360.0 / MAX_RESOLUTION
}

fn calibrated_percentage(position: i32, jc: &JointCalibration) -> f64 {
    let min = f64::from(jc.range_min);
    let max = f64::from(jc.range_max);
    let clamped = f64::from(position).clamp(min, max);
    let pct = (clamped - min) / (max - min) * 100.0;
    if jc.drive_mode != 0 {
        100.0 - pct
    } else {
        pct
    }
}

fn decode_sign_magnitude(raw: u16) -> i32 {
    let magnitude = (raw & 0x7FFF) as i32;
    if raw & 0x8000 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

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
                let raw_u16 = raw_position as u16;

                let decoded_position = decode_sign_magnitude(raw_u16);

                let calibrated_angle = self
                    .calibration
                    .as_ref()
                    .and_then(|cal| cal.get(joint.name()))
                    .map(|jc| {
                        if joint == Joint::Gripper {
                            calibrated_percentage(decoded_position, jc)
                        } else {
                            calibrated_degrees(decoded_position, jc)
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
