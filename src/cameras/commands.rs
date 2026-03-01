use serde::{Deserialize, Serialize};

use super::types::{CameraStatus, CameraStatusMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CameraCommand {
    Ping,
    Disconnect,
}

/// Parse a JSON text message into a `CameraCommand`.
pub fn parse_camera_command(data: &str) -> Result<CameraCommand, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("Invalid JSON: {}", e))?;

    serde_json::from_value(parsed).map_err(|e| format!("Invalid command: {}", e))
}

/// Handle a `CameraCommand`, returning an optional `CameraStatusMessage`
/// response. Returns `None` for commands that don't produce a response
/// (like `Disconnect`, which is handled by the caller breaking the loop).
pub fn handle_camera_command(cmd: &CameraCommand) -> Option<CameraStatusMessage> {
    match cmd {
        CameraCommand::Ping => Some(CameraStatusMessage::status(
            CameraStatus::Running,
            Some("pong".to_string()),
        )),
        CameraCommand::Disconnect => None,
    }
}
