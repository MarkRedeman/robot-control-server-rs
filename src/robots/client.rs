use poem_openapi::{Enum, Object};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Enum)]
pub enum Joint {
    Base,
    Shoulder,
    Elbow,
    WristPitch,
    WristRoll,
    Gripper,
}

impl Joint {
    pub const ALL: [Joint; 6] = [
        Joint::Base,
        Joint::Shoulder,
        Joint::Elbow,
        Joint::WristPitch,
        Joint::WristRoll,
        Joint::Gripper,
    ];

    pub fn motor_id(self) -> u8 {
        match self {
            Joint::Base => 1,
            Joint::Shoulder => 2,
            Joint::Elbow => 3,
            Joint::WristPitch => 4,
            Joint::WristRoll => 5,
            Joint::Gripper => 6,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Joint::Base => "shoulder_pan",
            Joint::Shoulder => "shoulder_lift",
            Joint::Elbow => "elbow_flex",
            Joint::WristPitch => "wrist_flex",
            Joint::WristRoll => "wrist_roll",
            Joint::Gripper => "gripper",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Object)]
pub struct JointState {
    pub joint: String,
    pub motor_id: u8,
    pub raw_position: i16,
    pub calibrated_angle: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Object)]
pub struct ArmState {
    pub timestamp: f64,
    pub joints: Vec<JointState>,
}

pub trait RobotClient: Send + Sync {
    fn port(&self) -> &str;
    fn is_connected(&self) -> bool;

    /// Read current arm state. Always returns both raw and calibrated values
    /// (when calibration data is available).
    fn read_state(&self) -> Result<ArmState, anyhow::Error>;

    /// Set joint positions using calibrated values. The values are unnormalized
    /// to raw servo positions using calibration data internally.
    fn set_joints_state(
        &self,
        positions: std::collections::HashMap<String, f64>,
    ) -> Result<ArmState, anyhow::Error>;

    fn enable_torque(&self) -> Result<serde_json::Value, anyhow::Error>;
    fn disable_torque(&self) -> Result<serde_json::Value, anyhow::Error>;
}
