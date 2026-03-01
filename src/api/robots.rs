use std::collections::HashMap;

use poem::web::Data;
use poem_openapi::{
    param::{Path, Query},
    payload::Json,
    Object, OpenApi,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::robots::{RobotCommand, RobotResponse};
use crate::state::{AppState, RobotInfo};

#[derive(Serialize, Object)]
pub struct RobotStateResponse {
    pub serial_id: String,
    pub port: String,
    pub is_connected: bool,
    pub is_controlled: bool,
    pub use_count: u32,
    // Using HashMap for flat state to match Python
    pub state: HashMap<String, f64>,
}

#[derive(Serialize, Object)]
pub struct RobotFullStateResponse {
    pub serial_id: String,
    pub state: crate::robots::ArmState,
}

#[derive(Deserialize, Object)]
pub struct SetJointsRequest {
    pub joints: HashMap<String, f64>,
}

#[derive(Serialize, Object)]
pub struct CommandResponse {
    pub success: bool,
    pub message: String,
}

pub struct RobotsApi;

#[OpenApi(prefix_path = "/api")]
impl RobotsApi {
    /// List of connected robots
    #[oai(path = "/robots", method = "get")]
    async fn list_robots(&self, state: Data<&AppState>) -> Json<Vec<RobotInfo>> {
        tracing::info!("API: list_robots");
        Json(state.list_robots())
    }

    /// Current arm state
    #[oai(path = "/robots/:serial_id", method = "get")]
    async fn get_robot_state(
        &self,
        state: Data<&AppState>,
        serial_id: Path<String>,
        #[oai(default = "default_normalize")] normalize: Query<bool>,
    ) -> Result<Json<RobotStateResponse>, AppError> {
        tracing::info!("API: get_robot_state called for serial_id={}", serial_id.0);

        state
            .get_or_create_robot(&serial_id.0, None)
            .map_err(AppError::robot_not_found)?;

        let handle = state
            .get_robot_handle(&serial_id.0)
            .map_err(AppError::robot_not_found)?;

        tracing::info!("API: robot created/retrieved, reading state");

        let response = handle
            .send_command(RobotCommand::ReadState)
            .await
            .map_err(|e| AppError::internal("worker_error", e))?;

        let arm_state = match response {
            RobotResponse::State { state: s } => s,
            RobotResponse::Error { error, message } => {
                return Err(AppError::internal(&error, message));
            }
            other => {
                return Err(AppError::internal(
                    "unexpected_response",
                    format!("Unexpected response: {:?}", other),
                ));
            }
        };

        let flat_state = arm_state.to_flat_state(normalize.0);

        tracing::info!("API: state read successfully");

        let (port, is_connected) = state
            .get_robot_info(&serial_id.0)
            .unwrap_or_else(|| ("unknown".to_string(), false));

        Ok(Json(RobotStateResponse {
            serial_id: serial_id.0,
            port,
            is_connected,
            is_controlled: true, // TODO: proper state tracking
            use_count: 1,
            state: flat_state,
        }))
    }

    /// Full arm state (legacy format)
    #[oai(path = "/robots/:serial_id/state", method = "get")]
    async fn get_robot_state_full(
        &self,
        state: Data<&AppState>,
        serial_id: Path<String>,
    ) -> Result<Json<RobotFullStateResponse>, AppError> {
        state
            .get_or_create_robot(&serial_id.0, None)
            .map_err(AppError::robot_not_found)?;

        let handle = state
            .get_robot_handle(&serial_id.0)
            .map_err(AppError::robot_not_found)?;

        let response = handle
            .send_command(RobotCommand::ReadState)
            .await
            .map_err(|e| AppError::internal("worker_error", e))?;

        let arm_state = match response {
            RobotResponse::State { state: s } => s,
            RobotResponse::Error { error, message } => {
                return Err(AppError::internal(&error, message));
            }
            other => {
                return Err(AppError::internal(
                    "unexpected_response",
                    format!("Unexpected response: {:?}", other),
                ));
            }
        };

        Ok(Json(RobotFullStateResponse {
            serial_id: serial_id.0,
            state: arm_state,
        }))
    }

    /// Send command to robot
    #[oai(path = "/robots/:serial_id/command", method = "post")]
    async fn send_command(
        &self,
        state: Data<&AppState>,
        serial_id: Path<String>,
        req: Json<SetJointsRequest>,
    ) -> Result<Json<CommandResponse>, AppError> {
        state
            .get_or_create_robot(&serial_id.0, None)
            .map_err(AppError::robot_not_found)?;

        let handle = state
            .get_robot_handle(&serial_id.0)
            .map_err(AppError::robot_not_found)?;

        let cmd = RobotCommand::SetJointsState {
            joints: req.0.joints,
        };

        let response = handle
            .send_command(cmd)
            .await
            .map_err(|e| AppError::internal("worker_error", e))?;

        if let RobotResponse::Error { error, message } = response {
            return Ok(Json(CommandResponse {
                success: false,
                message: format!("{}: {}", error, message),
            }));
        }

        Ok(Json(CommandResponse {
            success: true,
            message: "Command sent".to_string(),
        }))
    }
}

fn default_normalize() -> bool {
    true
}
