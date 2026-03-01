use poem::web::Data;
use poem_openapi::{Object, OpenApi, payload::Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize, Object)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Serialize, Object)]
pub struct ReadyResponse {
    pub status: String,
    pub robots_connected: usize,
}

#[derive(Serialize, Object)]
pub struct DebugPortsResponse {
    pub ports: Vec<PortInfo>,
}

#[derive(Serialize, Object)]
pub struct PortInfo {
    pub name: String,
    pub serial_number: Option<String>,
}

pub struct HealthApi;

#[OpenApi]
impl HealthApi {
    /// Health check
    #[oai(path = "/health", method = "get")]
    async fn health(&self) -> Json<HealthResponse> {
        Json(HealthResponse {
            status: "ok".to_string(),
        })
    }

    /// Readiness check
    #[oai(path = "/ready", method = "get")]
    async fn ready(&self, state: Data<&AppState>) -> Json<ReadyResponse> {
        let robots = state.list_robots();
        let connected = robots
            .iter()
            .filter(|r| r.client.as_ref().map(|c| c.is_connected).unwrap_or(false))
            .count();

        Json(ReadyResponse {
            status: "ok".to_string(),
            robots_connected: connected,
        })
    }

    /// Debug available ports
    #[oai(path = "/debug/ports", method = "get")]
    async fn debug_ports(&self) -> Json<DebugPortsResponse> {
        tracing::info!("debug_ports: calling available_ports");
        let result = serialport::available_ports();
        tracing::info!("debug_ports: result = {:?}", result);
        let ports: Vec<PortInfo> = result
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let serial_number = match p.port_type {
                    serialport::SerialPortType::UsbPort(usb) => usb.serial_number,
                    _ => None,
                };
                PortInfo {
                    name: p.port_name,
                    serial_number,
                }
            })
            .collect();
        Json(DebugPortsResponse { ports })
    }
}
