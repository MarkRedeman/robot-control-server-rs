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

    if let Err(e) = state.get_or_create_robot(&serial_id, query.calibration_id.as_deref()) {
        tracing::error!("Failed to get robot: {}", e);
        return poem::http::StatusCode::NOT_FOUND.into_response();
    }

    let client = {
        match state.robots.lock() {
            Ok(robots) => match robots.get(&serial_id) {
                Some(entry) => entry.client.clone(),
                None => return poem::http::StatusCode::NOT_FOUND.into_response(),
            },
            Err(_) => return poem::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    };

    ws.on_upgrade(move |socket| {
        handle_socket(socket, serial_id, client, fps_val, normalize)
    })
    .into_response()
}

async fn handle_socket(
    mut socket: WebSocketStream,
    serial_id: String,
    client: Arc<dyn RobotClient>,
    fps: u32,
    normalize: bool,
) {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let read_interval = Duration::from_millis(1000 / fps as u64);

    let client_read = client.clone();
    let tx_clone = tx.clone();
    let serial_id_clone = serial_id.clone();

    tokio::spawn(async move {
        let mut tick_interval = interval(read_interval);

        loop {
            tick_interval.tick().await;

            let state_result = tokio::task::spawn_blocking({
                let client = client_read.clone();
                move || client.read_state()
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("Task error: {}", e)));

            match state_result {
                Ok(state) => {
                    let response = RobotResponse::StateWasUpdated {
                        timestamp: state.timestamp,
                        state: state.to_flat_state(normalize),
                        is_controlled: true, // TODO: track actual torque state
                    };
                    let json = serde_json::to_string(&response).unwrap_or_default();
                    if tx_clone.send(json).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("read_state failed for {}: {}", serial_id_clone, e);
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
                            Ok(command) => command,
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
                            move || handle_command(client.as_ref(), cmd)
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
