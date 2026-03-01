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

use crate::robots::{parse_command, RobotResponse, RobotWorkerHandle};
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
    let normalize = query.normalize.unwrap_or(true);

    if let Err(e) = state.get_or_create_robot(&serial_id, query.calibration_id.as_deref()) {
        tracing::error!("Failed to get robot: {}", e);
        return poem::http::StatusCode::NOT_FOUND.into_response();
    }

    let handle = match state.get_robot_handle(&serial_id) {
        Ok(h) => h,
        Err(_) => return poem::http::StatusCode::NOT_FOUND.into_response(),
    };

    ws.on_upgrade(move |socket| handle_socket(socket, serial_id, handle, normalize))
        .into_response()
}

/// Manages the WebSocket session for a single robot.
///
/// Architecture:
///   - **Writer task** — owns the sink half of the socket and drains an mpsc
///     channel, sending each message to the client.
///   - **State forwarder task** — takes polled `ArmState` snapshots from the
///     worker's `state_rx` channel, converts them to `StateWasUpdated` JSON,
///     and pushes them into the writer channel.
///   - **Command loop** (this function) — reads the stream half. Incoming
///     text messages are parsed as `RobotCommand`s, sent to the worker via
///     `send_command()`, and the response is pushed into the writer channel.
async fn handle_socket(
    socket: WebSocketStream,
    serial_id: String,
    handle: RobotWorkerHandle,
    normalize: bool,
) {
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel::<String>(100);

    // --- Writer task: channel → sink ---
    let writer_handle = tokio::spawn(write_loop(sink, rx));

    // --- State forwarder task: worker state_rx → channel ---
    let state_forwarder = if let Some(mut state_rx) = handle.take_state_rx() {
        let tx_state = tx.clone();
        Some(tokio::spawn(async move {
            while let Some(state) = state_rx.recv().await {
                let response = RobotResponse::StateWasUpdated {
                    timestamp: state.timestamp,
                    state: state.to_flat_state(normalize),
                    is_controlled: true, // TODO: track actual torque state
                };
                let json = serde_json::to_string(&response).unwrap_or_default();
                if tx_state.send(json).await.is_err() {
                    break;
                }
            }
        }))
    } else {
        tracing::warn!(
            "state_rx already taken for robot {} — no state polling for this WS session",
            serial_id
        );
        None
    };

    // --- Command loop: stream → worker → channel ---
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let cmd = match parse_command(&text) {
                    Ok(command) => command,
                    Err(e) => {
                        let resp = RobotResponse::Error {
                            error: "parse_error".to_string(),
                            message: e,
                        };
                        let json = serde_json::to_string(&resp).unwrap_or_default();
                        if tx.send(json).await.is_err() {
                            break;
                        }
                        continue;
                    }
                };

                let response = match handle.send_command(cmd).await {
                    Ok(resp) => resp,
                    Err(e) => RobotResponse::Error {
                        error: "worker_error".to_string(),
                        message: e,
                    },
                };

                let json = serde_json::to_string(&response).unwrap_or_default();
                if tx.send(json).await.is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    // Client disconnected — shut everything down.
    drop(tx);
    if let Some(forwarder) = state_forwarder {
        forwarder.abort();
        let _ = forwarder.await;
    }
    let _ = writer_handle.await;

    tracing::info!("WebSocket session closed for robot {}", serial_id);
}

/// Drains the outgoing message channel and writes each message to the sink.
async fn write_loop(
    mut sink: futures_util::stream::SplitSink<WebSocketStream, Message>,
    mut rx: mpsc::Receiver<String>,
) {
    while let Some(json) = rx.recv().await {
        if sink.send(Message::Text(json)).await.is_err() {
            break;
        }
    }
    let _ = sink.close().await;
}
