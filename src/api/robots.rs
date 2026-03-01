use std::collections::HashMap;

use poem::web::Data;
use poem_openapi::{
    param::{Path, Query},
    payload::Json,
    Object, OpenApi,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::robots::{handle_command, RobotCommand};
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

        let _snapshot = state
            .get_or_create_robot(&serial_id.0)
            .map_err(AppError::robot_not_found)?;

        tracing::info!("API: robot created/retrieved, reading state");

        let state_data = tokio::task::spawn_blocking({
            let state = state.0.clone();
            let serial_id = serial_id.0.clone();
            move || state.get_robot_state(&serial_id)
        })
        .await
        .map_err(|e| AppError::internal("task_error", e.to_string()))?
        .map_err(AppError::robot_not_found)?;

        let mut flat_state = HashMap::new();
        for joint in &state_data.joints {
            let val = if normalize.0 {
                joint.calibrated_angle.unwrap_or_else(|| f64::from(joint.raw_position))
            } else {
                f64::from(joint.raw_position)
            };
            flat_state.insert(joint.joint.clone(), val);
        }

        tracing::info!("API: state read successfully");

        // Obtain use count and port from state
        let (port, use_count, is_connected) = {
            if let Ok(robots) = state.robots.lock() {
                if let Some(entry) = robots.get(&serial_id.0) {
                    (
                        entry.client.port().to_string(),
                        1,
                        entry.client.is_connected(),
                    )
                } else {
                    ("unknown".to_string(), 0, false)
                }
            } else {
                ("unknown".to_string(), 0, false)
            }
        };

        Ok(Json(RobotStateResponse {
            serial_id: serial_id.0,
            port,
            is_connected,
            is_controlled: true, // TODO: proper state tracking
            use_count,
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
        let _snapshot = state
            .get_or_create_robot(&serial_id.0)
            .map_err(AppError::robot_not_found)?;

        let state_data = tokio::task::spawn_blocking({
            let state = state.0.clone();
            let serial_id = serial_id.0.clone();
            move || state.get_robot_state(&serial_id)
        })
        .await
        .map_err(|e| AppError::internal("task_error", e.to_string()))?
        .map_err(AppError::robot_not_found)?;

        Ok(Json(RobotFullStateResponse {
            serial_id: serial_id.0,
            state: state_data,
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
        let _snapshot = state
            .get_or_create_robot(&serial_id.0)
            .map_err(AppError::robot_not_found)?;

        let client = {
            let robots = state.robots.lock().map_err(|_| {
                AppError::robot_not_found_with_code("lock_error", "lock error".to_string())
            })?;
            let entry = robots
                .get(&serial_id.0)
                .ok_or_else(|| AppError::robot_not_found(serial_id.0.clone()))?;
            entry.client.clone()
        };

        let cmd = RobotCommand::SetJointsState {
            joints: req.0.joints,
        };
        let response = tokio::task::spawn_blocking(move || {
            handle_command(client.as_ref(), cmd)
        })
        .await
        .map_err(|e| AppError::internal("task_error", e.to_string()))?;

        match response {
            crate::robots::RobotResponse::Error { error, message } => {
                return Ok(Json(CommandResponse {
                    success: false,
                    message: format!("{}: {}", error, message),
                }));
            }
            _ => {}
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
