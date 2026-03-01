use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use poem::{
    handler,
    web::{
        websocket::{Message, WebSocket, WebSocketStream},
        Query,
    },
    IntoResponse, Response,
};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::cameras::commands::{handle_camera_command, parse_camera_command, CameraCommand};
use crate::cameras::{types::CameraConfig, CameraStatusMessage, CameraWorker};

#[derive(Deserialize)]
pub struct CameraQuery {
    pub camera: String,
}

#[handler]
pub async fn camera_ws(ws: WebSocket, Query(query): Query<CameraQuery>) -> Response {
    let config: CameraConfig = match serde_json::from_str(&query.camera) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to parse camera config: {}", e);
            return poem::http::StatusCode::BAD_REQUEST.into_response();
        }
    };

    tracing::info!("Camera WebSocket request: {:?}", config);

    ws.on_upgrade(move |socket| handle_camera_socket(socket, config))
        .into_response()
}

async fn handle_camera_socket(mut socket: WebSocketStream, config: CameraConfig) {
    let (frame_tx, mut frame_rx) = mpsc::channel::<Result<Vec<u8>, anyhow::Error>>(2);
    let (status_tx, mut status_rx) = mpsc::channel::<crate::cameras::types::CameraStatus>(10);

    let mut worker = CameraWorker::new();

    if let Err(e) = worker.start(config, frame_tx, status_tx).await {
        tracing::error!("Failed to start camera worker: {}", e);
        return;
    }

    loop {
        tokio::select! {
            Some(frame_result) = frame_rx.recv() => {
                match frame_result {
                    Ok(frame) => {
                        tracing::debug!("Sending frame: {} bytes", frame.len());
                        if socket.send(Message::Binary(frame)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Frame error: {}", e);
                        let status = CameraStatusMessage::status(
                            crate::cameras::types::CameraStatus::Error,
                            Some(e.to_string()),
                        );
                        let json = serde_json::to_string(&status).unwrap_or_default();
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Some(status_msg) = status_rx.recv() => {
                let json = serde_json::to_string(&status_msg).unwrap_or_default();
                if socket.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match parse_camera_command(&text) {
                            Ok(cmd) => {
                                if matches!(cmd, CameraCommand::Disconnect) {
                                    tracing::info!("Client requested disconnect");
                                    break;
                                }
                                if let Some(response) = handle_camera_command(&cmd) {
                                    let json = serde_json::to_string(&response).unwrap_or_default();
                                    let _ = socket.send(Message::Text(json)).await;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Invalid camera command: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    worker.stop().await;
    tracing::info!("Camera WebSocket session closed");
}
