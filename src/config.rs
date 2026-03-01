use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_calibration_path")]
    pub calibration_path: PathBuf,

    #[serde(default = "default_fps")]
    pub default_fps: u32,

    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8000
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_calibration_path() -> PathBuf {
    PathBuf::from("../calibration/leader.json")
}

fn default_fps() -> u32 {
    30
}

fn default_baud_rate() -> u32 {
    1_000_000
}

impl Settings {
    pub fn from_env() -> Self {
        let host = std::env::var("HOST").unwrap_or_else(|_| default_host());
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(default_port);
        let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| default_log_level());
        let calibration_path = std::env::var("CALIBRATION_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_calibration_path());
        let default_fps = std::env::var("DEFAULT_FPS")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(default_fps);
        let baud_rate = std::env::var("BAUD_RATE")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(default_baud_rate);

        Self {
            host,
            port,
            log_level,
            calibration_path,
            default_fps,
            baud_rate,
        }
    }

    pub fn address(&self) -> SocketAddr {
        let addr: SocketAddr = format!("{}:{}", self.host, self.port)
            .parse()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 8000)));
        addr
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            log_level: default_log_level(),
            calibration_path: default_calibration_path(),
            default_fps: default_fps(),
            baud_rate: default_baud_rate(),
        }
    }
}
