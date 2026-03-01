//! Background worker that owns a [`RobotClient`] on a dedicated OS thread.
//!
//! The worker serialises all access to the robot (both periodic state reads
//! and on-demand commands) on a single `std::thread`, mirroring the pattern
//! used by [`CameraWorker`](crate::cameras::CameraWorker).
//!
//! Consumers interact through a [`RobotWorkerHandle`] which exposes:
//!
//! * **`send_command()`** — send a [`RobotCommand`] and `await` (or
//!   `blocking_recv`) the [`RobotResponse`] via a one-shot channel.
//! * **`state_rx`** — a channel that receives [`ArmState`] snapshots at the
//!   configured polling rate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};

use super::client::{ArmState, RobotClient};
use super::commands::{handle_command, RobotCommand, RobotResponse};

/// A command envelope sent to the worker thread.
struct Envelope {
    command: RobotCommand,
    reply: oneshot::Sender<RobotResponse>,
}

/// Configuration for creating a [`RobotWorker`].
pub struct RobotWorkerConfig {
    /// Polling rate in frames per second (clamped to 1..=240).
    pub fps: u32,
}

impl Default for RobotWorkerConfig {
    fn default() -> Self {
        Self { fps: 30 }
    }
}

/// Handle held by consumers to communicate with a running worker.
///
/// Cheaply cloneable — all clones share the same command channel, state
/// receiver slot, and running flag. The state receiver can only be taken
/// once across all clones.
#[derive(Clone)]
pub struct RobotWorkerHandle {
    cmd_tx: mpsc::Sender<Envelope>,
    /// Receiver for polled [`ArmState`] snapshots. Wrapped in
    /// `Arc<Mutex<Option<…>>>` so the handle is `Clone`; only one consumer
    /// can take it via [`take_state_rx`](Self::take_state_rx).
    state_rx: Arc<Mutex<Option<mpsc::Receiver<ArmState>>>>,
    running: Arc<AtomicBool>,
}

impl RobotWorkerHandle {
    /// Send a command and asynchronously await the response.
    pub async fn send_command(&self, command: RobotCommand) -> Result<RobotResponse, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(Envelope {
                command,
                reply: reply_tx,
            })
            .await
            .map_err(|_| "worker thread is gone".to_string())?;
        reply_rx
            .await
            .map_err(|_| "worker dropped reply channel".to_string())
    }

    /// Send a command and **block** until the response arrives.
    ///
    /// This is safe to call outside a tokio runtime (e.g. from the CLI).
    pub fn send_command_blocking(&self, command: RobotCommand) -> Result<RobotResponse, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .blocking_send(Envelope {
                command,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread is gone".to_string())?;
        reply_rx
            .blocking_recv()
            .map_err(|_| "worker dropped reply channel".to_string())
    }

    /// Take the state receiver. Can only be called once across all clones;
    /// subsequent calls return `None`.
    pub fn take_state_rx(&self) -> Option<mpsc::Receiver<ArmState>> {
        self.state_rx.lock().ok()?.take()
    }

    /// Whether the worker thread is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Request the worker to stop. Non-blocking; the thread will finish its
    /// current iteration and exit.
    pub fn request_stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Spawn a worker thread for the given robot client.
///
/// Returns a [`RobotWorkerHandle`] through which the caller can send
/// commands and receive polled state updates.
pub fn spawn_worker(
    client: Arc<dyn RobotClient>,
    config: RobotWorkerConfig,
) -> RobotWorkerHandle {
    let fps = config.fps.clamp(1, 240);
    let poll_interval = Duration::from_millis(1000 / u64::from(fps));

    // Command channel (inbound to worker).
    let (cmd_tx, cmd_rx) = mpsc::channel::<Envelope>(64);

    // State channel (outbound from worker).
    let (state_tx, state_rx) = mpsc::channel::<ArmState>(2);

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    std::thread::Builder::new()
        .name("robot-worker".into())
        .spawn(move || {
            worker_loop(client.as_ref(), cmd_rx, state_tx, running_clone, poll_interval);
        })
        .expect("failed to spawn robot-worker thread");

    RobotWorkerHandle {
        cmd_tx,
        state_rx: Arc::new(Mutex::new(Some(state_rx))),
        running,
    }
}

/// The main loop running on the worker thread.
///
/// On each iteration it:
/// 1. Drains all pending commands (non-blocking) and executes them.
/// 2. If the poll interval has elapsed, reads servo state and sends it
///    to `state_tx`.
fn worker_loop(
    client: &dyn RobotClient,
    mut cmd_rx: mpsc::Receiver<Envelope>,
    state_tx: mpsc::Sender<ArmState>,
    running: Arc<AtomicBool>,
    poll_interval: Duration,
) {
    let mut last_poll = Instant::now() - poll_interval; // poll immediately on first tick

    while running.load(Ordering::SeqCst) {
        // --- 1. Drain pending commands ---
        while let Ok(envelope) = cmd_rx.try_recv() {
            let response = handle_command(client, envelope.command);
            // If the caller dropped their receiver, that's fine — just discard.
            let _ = envelope.reply.send(response);
        }

        // --- 2. Poll state if interval elapsed ---
        let now = Instant::now();
        if now.duration_since(last_poll) >= poll_interval {
            last_poll = now;

            match client.read_state() {
                Ok(state) => {
                    // Use try_send to avoid blocking the worker if the
                    // consumer is slow. Dropping a frame is preferable to
                    // stalling the command loop.
                    match state_tx.try_send(state) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            // Consumer is behind — drop this frame.
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            // Consumer is gone. Keep running so commands
                            // still work (e.g. REST API without a WS
                            // consumer).
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("robot-worker read_state failed: {}", e);
                }
            }
        }

        // --- 3. Sleep briefly to avoid busy-spinning ---
        // We sleep for a short fixed duration so commands are picked up
        // promptly while not burning CPU.
        std::thread::sleep(Duration::from_millis(1));
    }

    tracing::info!("robot-worker thread exiting");
}
