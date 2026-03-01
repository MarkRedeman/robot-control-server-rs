use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::client::RobotClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum RobotCommand {
    Ping,
    EnableTorque,
    DisableTorque,
    SetJointsState {
        #[serde(default = "default_positions")]
        joints: HashMap<String, f64>,
    },
}

fn default_positions() -> HashMap<String, f64> {
    HashMap::new()
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RobotResponse {
    Pong,
    TorqueWasEnabled,
    TorqueWasDisabled,
    JointsStateWasSet,
    BaseWasStopped,
    StateWasUpdated {
        timestamp: f64,
        state: HashMap<String, f64>,
        is_controlled: bool,
    },
    Error {
        error: String,
        message: String,
    },
}

pub fn parse_command(data: &str) -> Result<RobotCommand, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;

    serde_json::from_value(parsed).map_err(|e| format!("Invalid command: {}", e))
}

pub fn handle_command<Robot: RobotClient + ?Sized>(
    robot: &Robot,
    cmd: RobotCommand,
    normalize: bool,
) -> RobotResponse {
    match cmd {
        RobotCommand::Ping => RobotResponse::Pong,

        RobotCommand::EnableTorque => match robot.enable_torque() {
            Ok(_) => RobotResponse::TorqueWasEnabled,
            Err(e) => RobotResponse::Error {
                error: "enable_torque_failed".to_string(),
                message: e.to_string(),
            },
        },

        RobotCommand::DisableTorque => match robot.disable_torque() {
            Ok(_) => RobotResponse::TorqueWasDisabled,
            Err(e) => RobotResponse::Error {
                error: "disable_torque_failed".to_string(),
                message: e.to_string(),
            },
        },

        RobotCommand::SetJointsState { joints } => {
            match robot.set_joints_state(joints, normalize) {
                Ok(_) => RobotResponse::JointsStateWasSet,
                Err(e) => RobotResponse::Error {
                    error: "set_joints_failed".to_string(),
                    message: e.to_string(),
                },
            }
        }
    }
}
