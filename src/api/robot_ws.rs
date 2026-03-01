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
use tokio_util::sync::CancellationToken;

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

/// Manages the WebSocket session for a single robot.
///
/// Architecture:
///   - **Writer task** — owns the sink half of the socket and drains an mpsc
///     channel, sending each message to the client. Stops when the channel
///     closes or the sink errors.
///   - **Poller task** — reads servo state on a fixed interval and pushes
///     JSON-encoded `StateWasUpdated` messages into the channel. Cancelled
///     via a `CancellationToken` when the command loop exits.
///   - **Command loop** (this function) — reads the stream half. Incoming
///     text messages are parsed as `RobotCommand`s, executed, and the
///     response is pushed into the same channel. On client disconnect the
///     loop breaks, the token is cancelled, and the channel is dropped.
async fn handle_socket(
    socket: WebSocketStream,
    serial_id: String,
    client: Arc<dyn RobotClient>,
    fps: u32,
    normalize: bool,
) {
    let token = CancellationToken::new();
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel::<String>(100);

    // --- Writer task: channel → sink ---
    let writer_handle = tokio::spawn(write_loop(sink, rx));

    // --- Poller task: servo → channel ---
    let poller_handle = tokio::spawn(poll_loop(
        client.clone(),
        tx.clone(),
        token.clone(),
        serial_id.clone(),
        fps,
        normalize,
    ));

    // --- Command loop: stream → handle → channel ---
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
                if tx.send(json).await.is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    // Client disconnected — shut everything down.
    token.cancel();
    drop(tx);
    let _ = poller_handle.await;
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

/// Reads servo state on a fixed interval and pushes updates into the channel.
async fn poll_loop(
    client: Arc<dyn RobotClient>,
    tx: mpsc::Sender<String>,
    token: CancellationToken,
    serial_id: String,
    fps: u32,
    normalize: bool,
) {
    let mut tick = interval(Duration::from_millis(1000 / u64::from(fps)));

    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            _ = tick.tick() => {}
        }

        let result = tokio::task::spawn_blocking({
            let client = client.clone();
            move || client.read_state()
        })
        .await
        .unwrap_or_else(|e| Err(anyhow::anyhow!("Task error: {}", e)));

        match result {
            Ok(state) => {
                let response = RobotResponse::StateWasUpdated {
                    timestamp: state.timestamp,
                    state: state.to_flat_state(normalize),
                    is_controlled: true, // TODO: track actual torque state
                };
                let json = serde_json::to_string(&response).unwrap_or_default();
                if tx.send(json).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::warn!("read_state failed for {}: {}", serial_id, e);
            }
        }
    }
}
