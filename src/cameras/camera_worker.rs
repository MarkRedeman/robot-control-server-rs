#![allow(deprecated)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use nokhwa::query;
use nokhwa::utils::{ApiBackend, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};

use crate::cameras::types::{CameraConfig, CameraInfo, CameraResolution, CameraStatus};

pub struct CameraWorker {
    running: Arc<AtomicBool>,
    stop_sender: Option<tokio::sync::mpsc::Sender<()>>,
}

impl Default for CameraWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraWorker {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            stop_sender: None,
        }
    }

    pub async fn start(
        &mut self,
        config: CameraConfig,
        frame_sender: tokio::sync::mpsc::Sender<Result<Vec<u8>, anyhow::Error>>,
        status_sender: tokio::sync::mpsc::Sender<CameraStatus>,
    ) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            tracing::warn!("Camera worker already running");
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);

        let (stop_tx, mut stop_rx) = tokio::sync::mpsc::channel::<()>(1);
        self.stop_sender = Some(stop_tx);

        let running = self.running.clone();

        std::thread::spawn(move || {
            let fingerprint = config.fingerprint.parse::<u32>().unwrap_or(0);
            let index = CameraIndex::Index(fingerprint);

            let width = config.width.unwrap_or(640);
            let height = config.height.unwrap_or(480);
            let fps = config.fps.unwrap_or(30);

            let mut camera = match nokhwa::Camera::new_with(
                index,
                width,
                height,
                fps,
                FrameFormat::MJPEG,
                ApiBackend::Auto,
            ) {
                Ok(cam) => cam,
                Err(e) => {
                    tracing::error!("Failed to create camera: {}", e);
                    status_sender.blocking_send(CameraStatus::Error).ok();
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            if let Err(e) = camera.open_stream() {
                tracing::error!("Failed to open camera stream: {}", e);
                status_sender.blocking_send(CameraStatus::Error).ok();
                running.store(false, Ordering::SeqCst);
                return;
            }

            tracing::info!("Camera started successfully");
            status_sender.blocking_send(CameraStatus::Running).ok();

            loop {
                if stop_rx.try_recv().is_ok() {
                    tracing::info!("Camera worker received stop signal");
                    break;
                }

                std::thread::sleep(std::time::Duration::from_millis(33));

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                match camera.frame() {
                    Ok(buffer) => match encode_to_jpeg(&buffer) {
                        Ok(jpeg_data) => {
                            if let Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) =
                                frame_sender.try_send(Ok(jpeg_data))
                            {
                                tracing::debug!("Frame receiver dropped, stopping camera");
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to encode frame: {}", e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Frame capture error: {}", e);
                    }
                }
            }

            status_sender.blocking_send(CameraStatus::Stopped).ok();
            running.store(false, Ordering::SeqCst);
            tracing::info!("Camera worker stopped");
        });

        Ok(())
    }

    pub async fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        tracing::info!("Stopping camera worker");

        if let Some(sender) = self.stop_sender.take() {
            sender.send(()).await.ok();
        }

        self.running.store(false, Ordering::SeqCst);
    }
}

fn encode_to_jpeg(buffer: &nokhwa::Buffer) -> Result<Vec<u8>> {
    use image::{ImageBuffer, RgbImage};

    let frame_format = buffer.source_frame_format();
    tracing::debug!("Buffer frame format: {:?}", frame_format);

    if frame_format == FrameFormat::MJPEG {
        let mut data = buffer.buffer();
        // V4L2 MJPEG buffers are often padded with zeros. Strip trailing zeros to ensure
        // strict decoders (like browser image tags) don't reject the JPEG.
        while let Some(&0) = data.last() {
            data = &data[..data.len() - 1];
        }
        return Ok(data.to_vec());
    }

    let resolution = buffer.resolution();
    let width = resolution.width();
    let height = resolution.height();
    let raw = buffer.buffer();

    tracing::debug!("Encoding frame: {}x{} ({} bytes)", width, height, raw.len());

    let img: RgbImage = ImageBuffer::from_raw(width, height, raw.to_vec())
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer from {} bytes", raw.len()))?;

    let mut jpeg_data = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_data);

    img.write_to(&mut cursor, image::ImageFormat::Jpeg)?;

    tracing::debug!("Encoded JPEG size: {} bytes", jpeg_data.len());

    Ok(jpeg_data)
}

pub fn list_cameras() -> Vec<CameraInfo> {
    let devices = match query(ApiBackend::Auto) {
        Ok(val) => val,
        Err(e) => {
            tracing::warn!("Failed to query cameras: {}", e);
            return vec![];
        }
    };

    devices
        .into_iter()
        .filter_map(|device| {
            let index = device.index();
            let idx = match *index {
                CameraIndex::Index(i) => i,
                _ => return None,
            };

            let requested = RequestedFormat::with_formats(
                RequestedFormatType::AbsoluteHighestFrameRate,
                &[FrameFormat::MJPEG, FrameFormat::YUYV],
            );

            match nokhwa::Camera::new(index.clone(), requested) {
                Ok(camera) => {
                    let fmt = camera.camera_format();
                    Some(CameraInfo {
                        driver: "usb_camera".to_string(),
                        fingerprint: idx.to_string(),
                        hardware_name: device.human_name(),
                        default_profile: CameraResolution {
                            width: fmt.width(),
                            height: fmt.height(),
                            fps: vec![30],
                        },
                    })
                }
                Err(_) => None,
            }
        })
        .collect()
}

pub fn get_supported_formats(fingerprint: &str) -> Option<Vec<CameraResolution>> {
    let index = fingerprint.parse::<u32>().unwrap_or(0);
    let camera_index = CameraIndex::Index(index);

    let requested = RequestedFormat::with_formats(
        RequestedFormatType::AbsoluteHighestFrameRate,
        &[FrameFormat::MJPEG, FrameFormat::YUYV],
    );

    let mut camera = match nokhwa::Camera::new(camera_index, requested) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to open camera for format query: {}", e);
            return None;
        }
    };

    let resolutions = match camera.compatible_list_by_resolution(FrameFormat::MJPEG) {
        Ok(res) => res,
        Err(e) => {
            tracing::warn!("Failed to get resolutions: {}", e);
            return None;
        }
    };

    Some(
        resolutions
            .into_iter()
            .map(|(res, fps_list)| CameraResolution {
                width: res.width(),
                height: res.height(),
                fps: fps_list,
            })
            .collect(),
    )
}
