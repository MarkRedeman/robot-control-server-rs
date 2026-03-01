use poem_openapi::{ApiResponse, Object, payload::Json};

#[derive(Object, Debug)]
pub struct ErrorPayload {
    pub error: String,
    pub message: String,
}

#[derive(ApiResponse, Debug)]
pub enum AppError {
    #[oai(status = 404)]
    RobotNotFound(Json<ErrorPayload>),

    #[oai(status = 503)]
    RobotNotConnected(Json<ErrorPayload>),

    #[oai(status = 400)]
    InvalidCommand(Json<ErrorPayload>),

    #[oai(status = 500)]
    SerialPort(Json<ErrorPayload>),

    #[oai(status = 500)]
    Calibration(Json<ErrorPayload>),

    #[oai(status = 500)]
    WebSocket(Json<ErrorPayload>),

    #[oai(status = 500)]
    Internal(Json<ErrorPayload>),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::RobotNotFound(payload) => write!(f, "Robot not found: {}", payload.0.message),
            AppError::RobotNotConnected(payload) => {
                write!(f, "Robot not connected: {}", payload.0.message)
            }
            AppError::InvalidCommand(payload) => {
                write!(f, "Invalid command: {}", payload.0.message)
            }
            AppError::SerialPort(payload) => write!(f, "Serial port error: {}", payload.0.message),
            AppError::Calibration(payload) => write!(f, "Calibration error: {}", payload.0.message),
            AppError::WebSocket(payload) => write!(f, "WebSocket error: {}", payload.0.message),
            AppError::Internal(payload) => write!(f, "Internal error: {}", payload.0.message),
        }
    }
}

impl std::error::Error for AppError {}

impl AppError {
    pub fn robot_not_found(msg: String) -> Self {
        AppError::RobotNotFound(Json(ErrorPayload {
            error: "robot_not_found".to_string(),
            message: msg,
        }))
    }

    pub fn robot_not_found_with_code(code: &str, msg: String) -> Self {
        AppError::RobotNotFound(Json(ErrorPayload {
            error: code.to_string(),
            message: msg,
        }))
    }

    pub fn robot_not_connected(msg: String) -> Self {
        AppError::RobotNotConnected(Json(ErrorPayload {
            error: "robot_not_connected".to_string(),
            message: msg,
        }))
    }

    pub fn invalid_command(msg: String) -> Self {
        AppError::InvalidCommand(Json(ErrorPayload {
            error: "invalid_command".to_string(),
            message: msg,
        }))
    }

    pub fn internal(error_code: &str, msg: String) -> Self {
        AppError::Internal(Json(ErrorPayload {
            error: error_code.to_string(),
            message: msg,
        }))
    }
}
