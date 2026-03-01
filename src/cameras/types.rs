use poem_openapi::Object;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Object)]
pub struct CameraResolution {
    pub width: u32,
    pub height: u32,
    pub fps: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Object)]
pub struct CameraInfo {
    pub driver: String,
    pub fingerprint: String,
    pub hardware_name: String,
    pub default_profile: CameraResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize, Object)]
pub struct CameraConfig {
    pub driver: String,
    pub fingerprint: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub fps: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraStatus {
    Pending,
    Connecting,
    Running,
    Reconnecting,
    Error,
    Stopped,
}

impl CameraStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CameraStatus::Pending => "pending",
            CameraStatus::Connecting => "connecting",
            CameraStatus::Running => "running",
            CameraStatus::Reconnecting => "reconnecting",
            CameraStatus::Error => "error",
            CameraStatus::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraStatusMessage {
    pub event: String,
    pub state: String,
    #[serde(default)]
    pub message: Option<String>,
}

impl CameraStatusMessage {
    pub fn status(state: CameraStatus, message: Option<String>) -> Self {
        Self {
            event: "status".to_string(),
            state: state.as_str().to_string(),
            message,
        }
    }
}
