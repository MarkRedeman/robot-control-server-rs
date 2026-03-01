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
/// A single `tokio::select!` loop reads from both the worker's polled
/// state channel and the WebSocket stream, writing responses directly
/// to the socket. No intermediate channel or writer task needed.
async fn handle_socket(
    socket: WebSocketStream,
    serial_id: String,
    handle: RobotWorkerHandle,
    normalize: bool,
) {
    let mut state_rx = handle.take_state_rx();
    if state_rx.is_none() {
        tracing::warn!(
            "state_rx already taken for robot {} — no state polling for this WS session",
            serial_id
        );
    }

    // We can't split the socket because both branches of select! need
    // mutable access to write. Instead, use the unsplit socket directly
    // (same pattern as camera_ws).
    let (mut sink, mut stream) = socket.split();

    loop {
        tokio::select! {
            // --- Polled state from worker ---
            Some(state) = async {
                match state_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                let response = RobotResponse::StateWasUpdated {
                    timestamp: state.timestamp,
                    state: state.to_flat_state(normalize),
                    is_controlled: true, // TODO: track actual torque state
                };
                let json = serde_json::to_string(&response).unwrap_or_default();
                if sink.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }

            // --- Incoming WebSocket messages ---
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let response = match parse_command(&text) {
                            Ok(cmd) => match handle.send_command(cmd).await {
                                Ok(resp) => resp,
                                Err(e) => RobotResponse::Error {
                                    error: "worker_error".to_string(),
                                    message: e,
                                },
                            },
                            Err(e) => RobotResponse::Error {
                                error: "parse_error".to_string(),
                                message: e,
                            },
                        };
                        let json = serde_json::to_string(&response).unwrap_or_default();
                        if sink.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    let _ = sink.close().await;
    tracing::info!("WebSocket session closed for robot {}", serial_id);
}
