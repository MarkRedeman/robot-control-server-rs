use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use poem_openapi::Object;
use serde::{Deserialize, Serialize};

use crate::config::Settings;
use crate::robots::{load_calibration, ArmState, FeetechRobotClient, RobotClient};

#[derive(Debug, Clone, Serialize, Deserialize, Object)]
pub struct RobotClientInfo {
    pub is_connected: bool,
    pub use_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Object)]
pub struct RobotInfo {
    pub serial_id: String,
    pub port: String,
    pub description: String,
    pub manufacturer: String,
    pub client: Option<RobotClientInfo>,
}

pub struct RobotEntry {
    pub client: Arc<dyn RobotClient>,
}

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub robots: Arc<Mutex<HashMap<String, RobotEntry>>>,
    pub calibration: Option<serde_json::Value>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        let calibration = load_calibration(&settings.calibration_path)
            .ok()
            .map(|cal| serde_json::to_value(&cal).unwrap_or(serde_json::Value::Null));

        Self {
            settings,
            robots: Arc::new(Mutex::new(HashMap::new())),
            calibration,
        }
    }

    pub fn get_or_create_robot(&self, serial_id: &str) -> Result<(), String> {
        tracing::info!("get_or_create_robot: serial_id={}", serial_id);

        let mut robots = self.robots.lock().map_err(|e| e.to_string())?;

        if robots.contains_key(serial_id) {
            tracing::info!("Found existing robot for serial_id={}", serial_id);
            return Ok(());
        }

        tracing::info!("Creating new robot for serial_id={}", serial_id);

        let calibration = self
            .calibration
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let port = Self::find_port_by_serial(serial_id);
        tracing::info!("Found port {:?} for serial_id {}", port, serial_id);

        if port.is_none() {
            tracing::warn!("No port found for serial_id={}", serial_id);
        }

        let client = Arc::new(
            FeetechRobotClient::new(
                serial_id.to_string(),
                port.unwrap_or_else(|| serial_id.to_string()),
                self.settings.baud_rate,
                calibration,
            )
            .map_err(|e| e.to_string())?,
        ) as Arc<dyn RobotClient>;

        robots.insert(serial_id.to_string(), RobotEntry { client });

        tracing::info!("Created robot entry for serial_id={}", serial_id);
        Ok(())
    }

    fn find_port_by_serial(serial_id: &str) -> Option<String> {
        let ports = match serialport::available_ports() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to enumerate ports: {}", e);
                return None;
            }
        };

        for port in ports {
            if let serialport::SerialPortType::UsbPort(usb_info) = port.port_type {
                if usb_info.serial_number.as_deref() == Some(serial_id) {
                    return Some(port.port_name);
                }
            }
        }
        None
    }

    pub fn list_robots(&self) -> Vec<RobotInfo> {
        let ports = match serialport::available_ports() {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        let mut robots = Vec::new();
        let mut seen_serials: std::collections::HashSet<String> = std::collections::HashSet::new();

        let connected_serials: std::collections::HashSet<String> = self
            .robots
            .lock()
            .ok()
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default();

        for port in ports {
            let (serial_id, manufacturer) = match &port.port_type {
                serialport::SerialPortType::UsbPort(usb_info) => (
                    usb_info.serial_number.clone(),
                    usb_info.manufacturer.clone(),
                ),
                _ => (None, None),
            };

            let Some(serial_id) = serial_id else {
                continue;
            };

            if seen_serials.contains(&serial_id) {
                continue;
            }
            seen_serials.insert(serial_id.clone());

            let client_info = if connected_serials.contains(&serial_id) {
                Some(RobotClientInfo {
                    is_connected: true,
                    use_count: 1, // hardcoded for now
                })
            } else {
                None
            };

            robots.push(RobotInfo {
                serial_id,
                port: port.port_name,
                description: "USB Serial Device".to_string(),
                manufacturer: manufacturer.unwrap_or_else(|| "Unknown".to_string()),
                client: client_info,
            });
        }

        robots
    }

    pub fn get_robot_state(&self, serial_id: &str) -> Result<ArmState, String> {
        tracing::info!("get_robot_state: serial_id={}", serial_id);

        let robots = self.robots.lock().map_err(|e| e.to_string())?;

        let entry = robots.get(serial_id).ok_or_else(|| {
            tracing::error!("Robot not found in pool: {}", serial_id);
            format!("Robot not found: {}", serial_id)
        })?;

        tracing::info!("Found robot entry, calling read_state");
        entry.client.read_state().map_err(|e| {
            tracing::error!("read_state failed: {}", e);
            e.to_string()
        })
    }
}
