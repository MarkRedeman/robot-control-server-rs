use std::sync::Arc;

use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use poem::{
    handler,
    web::{
        websocket::{Message, WebSocket, WebSocketStream},
        Data, Path, Query,
    },
    IntoResponse, Response,
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::robots::{handle_command, parse_command, RobotClient, RobotResponse};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct WsQuery {
    pub fps: Option<u32>,
    pub driver: Option<String>,
    pub normalize: Option<bool>,
    pub calibration_id: Option<String>,
}

#[handler]
pub async fn robot_ws(
    ws: WebSocket,
    state: Data<&AppState>,
    Path(serial_id): Path<String>,
    Query(query): Query<WsQuery>,
) -> Response {
    let fps_val = query.fps.unwrap_or(30).clamp(1, 240);
    let normalize = query.normalize.unwrap_or(true);

    let snapshot = match state.get_or_create_robot(&serial_id) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get robot: {}", e);
            return poem::http::StatusCode::NOT_FOUND.into_response();
        }
    };

    let client = {
        match state.robots.lock() {
            Ok(robots) => match robots.get(&serial_id) {
                Some(e) => e.client.clone(),
                None => return poem::http::StatusCode::NOT_FOUND.into_response(),
            },
            Err(_) => return poem::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    };

    ws.on_upgrade(move |socket| {
        handle_socket(
            socket, serial_id, client, snapshot, fps_val, normalize 
        )
    })
    .into_response()
}

async fn handle_socket(
    mut socket: WebSocketStream,
    serial_id: String,
    client: Arc<dyn RobotClient>,
    snapshot: Arc<std::sync::Mutex<crate::state::RobotSnapshot>>,
    fps: u32,
    normalize: bool,
) {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let read_interval = Duration::from_millis(1000 / fps as u64);

    let client_read = client.clone();
    let snapshot_clone = snapshot.clone();
    let tx_clone = tx.clone();

    tokio::spawn(async move {
        let mut tick_interval = interval(read_interval);

        loop {
            tick_interval.tick().await;

            let state_result = tokio::task::spawn_blocking({
                let client = client_read.clone();
                move || client.read_state(normalize)
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("Task error: {}", e)));

            match state_result {
                Ok(state) => {
                    let mut flat_state = std::collections::HashMap::new();
                    for joint in &state.joints {
                        let val = if normalize {
                            joint.calibrated_angle.unwrap_or(joint.position_rad)
                        } else {
                            joint.position_deg // Or raw position? Python uses normalized = [-1, 1] or raw encoder
                        };
                        let joint_name =  joint.joint.clone();

                        flat_state.insert(joint_name, val);
                    }

                    let timestamp = match state.timestamp.parse::<f64>() {
                        Ok(t) => t,
                        Err(_) => chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                    };

                    let response = RobotResponse::StateWasUpdated {
                        timestamp,
                        state: flat_state,
                        is_controlled: true, // TODO: track actual torque state
                    };
                    let json = serde_json::to_string(&response).unwrap_or_default();
                    let _ = tx_clone.send(json).await;

                    if let Ok(mut snap) = snapshot_clone.lock() {
                        snap.state = Some(state);
                        snap.error = None;
                    }
                }
                Err(e) => {
                    if let Ok(mut snap) = snapshot_clone.lock() {
                        snap.error = Some(e.to_string());
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            Some(json) = rx.recv() => {
                if socket.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let cmd = match parse_command(&text) {
                            Ok(c) => c,
                            Err(e) => {
                                let error_response = RobotResponse::Error {
                                    error: "parse_error".to_string(),
                                    message: e,
                                };
                                let json = serde_json::to_string(&error_response).unwrap_or_default();
                                let _ = tx.send(json).await;
                                continue;
                            }
                        };

                        let response = tokio::task::spawn_blocking({
                            let client = client.clone();

                            move || handle_command(client.as_ref(), cmd, normalize )
                        })
                        .await
                        .unwrap_or_else(|e| RobotResponse::Error {
                            error: "task_error".to_string(),
                            message: e.to_string(),
                        });
                        let json = serde_json::to_string(&response).unwrap_or_default();
                        let _ = tx.send(json).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    tracing::info!("WebSocket session closed for robot {}", serial_id);
}
