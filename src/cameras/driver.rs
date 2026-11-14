//! Synchronous camera driver used by the Python bindings.
//!
//! The tokio [`CameraWorker`](crate::cameras::CameraWorker) used by the HTTP
//! server streams JPEG frames; this module instead exposes a simple blocking
//! RGB reader that maps directly onto the PhysicalAI `Camera` interface.

use anyhow::Result;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};

/// A decoded RGB frame.
#[derive(Debug, Clone)]
pub struct FrameData {
    pub width: u32,
    pub height: u32,
    /// RGB8 pixels, `width * height * 3` bytes.
    pub data: Vec<u8>,
}

/// Metadata about a discovered camera device.
#[derive(Debug, Clone)]
pub struct CameraDeviceInfo {
    pub device_id: String,
    pub index: u32,
    pub name: String,
}

/// A single supported ``(width, height, fps)`` format.
#[derive(Debug, Clone)]
pub struct CameraFormat {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// Structural interface for a live camera, mirroring
/// ``physicalai.capture.Camera``.
pub trait CameraDriver {
    fn device_id(&self) -> String;
    fn connect(&mut self) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
    fn read(&mut self) -> Result<FrameData>;
    fn read_latest(&mut self) -> Result<FrameData>;
}

/// A UVC camera backed by nokhwa.
pub struct NokhwaCamera {
    index: u32,
    width: u32,
    height: u32,
    fps: u32,
    camera: Option<nokhwa::Camera>,
}

impl NokhwaCamera {
    pub fn new(index: u32, width: u32, height: u32, fps: u32) -> Self {
        Self {
            index,
            width,
            height,
            fps,
            camera: None,
        }
    }
}

impl CameraDriver for NokhwaCamera {
    fn device_id(&self) -> String {
        format!("/dev/video{}", self.index)
    }

    fn connect(&mut self) -> Result<()> {
        if self.camera.is_some() {
            return Ok(());
        }

        let camera_format = nokhwa::utils::CameraFormat::new_from(
            self.width,
            self.height,
            FrameFormat::MJPEG,
            self.fps,
        );
        let requested = RequestedFormat::with_formats(
            RequestedFormatType::Exact(camera_format),
            &[FrameFormat::MJPEG],
        );
        let mut camera = nokhwa::Camera::new(CameraIndex::Index(self.index), requested)?;
        camera.open_stream()?;
        self.camera = Some(camera);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        if let Some(mut camera) = self.camera.take() {
            camera.stop_stream()?;
        }
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.camera.is_some()
    }

    fn read(&mut self) -> Result<FrameData> {
        self.read_latest()
    }

    fn read_latest(&mut self) -> Result<FrameData> {
        let camera = self
            .camera
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Camera is not connected. Call connect() first."))?;
        let buffer = camera.frame()?;
        decode_rgb(&buffer)
    }
}

fn decode_rgb(buffer: &nokhwa::Buffer) -> Result<FrameData> {
    let image = buffer.decode_image::<RgbFormat>()?;
    Ok(FrameData {
        width: image.width(),
        height: image.height(),
        data: image.into_raw(),
    })
}

/// List available UVC camera devices.
pub fn discover() -> Vec<CameraDeviceInfo> {
    let devices = match nokhwa::query(ApiBackend::Auto) {
        Ok(val) => val,
        Err(e) => {
            tracing::warn!("Failed to query cameras: {}", e);
            return Vec::new();
        }
    };

    devices
        .into_iter()
        .filter_map(|device| {
            let idx = match *device.index() {
                CameraIndex::Index(i) => i,
                _ => return None,
            };
            Some(CameraDeviceInfo {
                device_id: format!("/dev/video{idx}"),
                index: idx,
                name: device.human_name(),
            })
        })
        .collect()
}

/// Query supported ``(width, height, fps)`` formats for a camera index.
pub fn query_formats(index: u32) -> Vec<CameraFormat> {
    let requested = RequestedFormat::with_formats(
        RequestedFormatType::AbsoluteHighestFrameRate,
        &[FrameFormat::MJPEG, FrameFormat::YUYV],
    );

    let mut camera = match nokhwa::Camera::new(CameraIndex::Index(index), requested) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to open camera for format query: {}", e);
            return Vec::new();
        }
    };

    let resolutions = match camera.compatible_list_by_resolution(FrameFormat::MJPEG) {
        Ok(res) => res,
        Err(e) => {
            tracing::warn!("Failed to get resolutions: {}", e);
            return Vec::new();
        }
    };

    let mut formats = Vec::new();
    for (res, fps_list) in resolutions {
        for fps in fps_list {
            formats.push(CameraFormat {
                width: res.width(),
                height: res.height(),
                fps,
            });
        }
    }
    formats.sort_by_key(|f| (f.width, f.height, f.fps));
    formats
}
