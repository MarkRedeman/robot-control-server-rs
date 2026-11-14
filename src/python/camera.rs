//! PyO3 binding for the synchronous nokhwa camera driver.
//!
//! `nokhwa::Camera` is `!Send`, so it cannot be held directly on a Python
//! object that crosses threads, and it cannot be used inside
//! [`Python::allow_threads`] (which requires the closure to be `Send`). We
//! instead own the camera on a dedicated background thread and communicate via
//! `std::sync::mpsc` channels, which keeps the GIL free during blocking reads.

use std::sync::mpsc;

use numpy::PyArray3;
use numpy::ndarray::Array3;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::cameras::{CameraDriver, FrameData, NokhwaCamera};

enum CameraCommand {
    Connect {
        width: u32,
        height: u32,
        fps: u32,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Disconnect {
        reply: mpsc::Sender<Result<(), String>>,
    },
    Read {
        reply: mpsc::Sender<Result<FrameData, String>>,
    },
    IsConnected {
        reply: mpsc::Sender<bool>,
    },
}

fn camera_loop(rx: mpsc::Receiver<CameraCommand>, index: u32) {
    let mut camera: Option<NokhwaCamera> = None;

    for command in rx {
        match command {
            CameraCommand::Connect {
                width,
                height,
                fps,
                reply,
            } => {
                let result = (|| -> Result<(), String> {
                    if camera.is_some() {
                        return Ok(());
                    }
                    let mut c = NokhwaCamera::new(index, width, height, fps);
                    c.connect().map_err(|e| e.to_string())?;
                    camera = Some(c);
                    Ok(())
                })();
                let _ = reply.send(result);
            }
            CameraCommand::Disconnect { reply } => {
                let result = match camera.take() {
                    Some(mut c) => c.disconnect().map_err(|e| e.to_string()),
                    None => Ok(()),
                };
                let _ = reply.send(result);
            }
            CameraCommand::Read { reply } => {
                let result = match camera.as_mut() {
                    None => Err("Camera is not connected. Call connect() first.".to_string()),
                    Some(c) => c.read().map_err(|e| e.to_string()),
                };
                let _ = reply.send(result);
            }
            CameraCommand::IsConnected { reply } => {
                let _ = reply.send(camera.is_some());
            }
        }
    }
}

fn channel_err(e: mpsc::SendError<CameraCommand>) -> PyErr {
    PyRuntimeError::new_err(format!("camera worker is gone: {e}"))
}

fn recv_err(e: mpsc::RecvError) -> PyErr {
    PyRuntimeError::new_err(format!("camera worker is gone: {e}"))
}

/// A Rust-backed UVC camera.
#[pyclass(name = "RustCamera")]
pub struct RustCamera {
    cmd_tx: mpsc::Sender<CameraCommand>,
    index: u32,
    width: u32,
    height: u32,
    fps: u32,
}

#[pymethods]
impl RustCamera {
    #[new]
    fn new(index: u32, width: u32, height: u32, fps: u32) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("rust-camera".into())
            .spawn(move || camera_loop(cmd_rx, index))
            .expect("failed to spawn rust-camera thread");
        Self {
            cmd_tx,
            index,
            width,
            height,
            fps,
        }
    }

    fn device_id(&self) -> String {
        format!("/dev/video{}", self.index)
    }

    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        let (reply, rx) = mpsc::channel();
        self.cmd_tx
            .send(CameraCommand::Connect {
                width: self.width,
                height: self.height,
                fps: self.fps,
                reply,
            })
            .map_err(channel_err)?;
        py.allow_threads(move || rx.recv())
            .map_err(recv_err)?
            .map_err(PyRuntimeError::new_err)
    }

    fn disconnect(&self, py: Python<'_>) -> PyResult<()> {
        let (reply, rx) = mpsc::channel();
        self.cmd_tx
            .send(CameraCommand::Disconnect { reply })
            .map_err(channel_err)?;
        py.allow_threads(move || rx.recv())
            .map_err(recv_err)?
            .map_err(PyRuntimeError::new_err)
    }

    fn is_connected(&self, py: Python<'_>) -> bool {
        let (reply, rx) = mpsc::channel();
        if self
            .cmd_tx
            .send(CameraCommand::IsConnected { reply })
            .is_err()
        {
            return false;
        }
        py.allow_threads(move || rx.recv()).unwrap_or(false)
    }

    /// Read the next frame as an ``(H, W, 3)`` uint8 RGB array.
    fn read<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray3<u8>>> {
        self.read_latest(py)
    }

    /// Read the most recent frame (best-effort, mirrors :meth:`read`).
    fn read_latest<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray3<u8>>> {
        let (reply, rx) = mpsc::channel();
        self.cmd_tx
            .send(CameraCommand::Read { reply })
            .map_err(channel_err)?;
        let frame = py
            .allow_threads(move || rx.recv())
            .map_err(recv_err)?
            .map_err(PyRuntimeError::new_err)?;
        frame_to_array(py, frame)
    }
}

fn frame_to_array<'py>(py: Python<'py>, frame: FrameData) -> PyResult<Bound<'py, PyArray3<u8>>> {
    let array =
        Array3::from_shape_vec((frame.height as usize, frame.width as usize, 3), frame.data)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(PyArray3::from_owned_array(py, array))
}
