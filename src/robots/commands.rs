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
    StopBase,
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

pub fn handle_command<R: RobotClient + ?Sized>(
    client: &R,
    cmd: RobotCommand,
    normalize: bool,
    robot_type: String,
) -> RobotResponse {
    match cmd {
        RobotCommand::Ping => RobotResponse::Pong,

        RobotCommand::EnableTorque => match client.enable_torque() {
            Ok(_) => RobotResponse::TorqueWasEnabled,
            Err(e) => RobotResponse::Error {
                error: "enable_torque_failed".to_string(),
                message: e.to_string(),
            },
        },

        RobotCommand::DisableTorque => match client.disable_torque() {
            Ok(_) => RobotResponse::TorqueWasDisabled,
            Err(e) => RobotResponse::Error {
                error: "disable_torque_failed".to_string(),
                message: e.to_string(),
            },
        },

        RobotCommand::SetJointsState { joints } => {
            let mut un_prefixed = HashMap::new();
            for (k, v) in joints {
                if robot_type == "lekiwi" && k.starts_with("arm_") {
                    un_prefixed.insert(k.strip_prefix("arm_").unwrap().to_string(), v);
                } else if robot_type == "lekiwi" && (k == "x" || k == "y" || k == "theta") {
                    // Ignored for now since we don't have mobile base hardware implementation
                } else {
                    un_prefixed.insert(k, v);
                }
            }

            match client.set_joints_state(un_prefixed, normalize) {
                Ok(_) => RobotResponse::JointsStateWasSet,
                Err(e) => RobotResponse::Error {
                    error: "set_joints_failed".to_string(),
                    message: e.to_string(),
                },
            }
        }

        RobotCommand::StopBase => {
            if robot_type == "lekiwi" {
                RobotResponse::BaseWasStopped
            } else {
                RobotResponse::Error {
                    error: "unsupported_command".to_string(),
                    message: "stop_base is only supported for mobile robots".to_string(),
                }
            }
        }
    }
}
