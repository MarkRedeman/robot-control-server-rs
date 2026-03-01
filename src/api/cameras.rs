use poem_openapi::payload::Json;
use poem_openapi::{Object, OpenApi};

use crate::cameras::{CameraInfo, CameraResolution, get_supported_formats, list_cameras};

#[derive(serde::Serialize, Object)]

pub struct CamerasApi;

#[OpenApi(prefix_path = "/api")]
impl CamerasApi {
    /// List available cameras
    #[oai(path = "/cameras", method = "get")]
    async fn list_cameras(&self) -> Json<Vec<CameraInfo>> {
        tracing::info!("API: list_cameras");
        Json(list_cameras())
    }

    /// Get supported formats for a camera
    #[oai(
        path = "/cameras/supported_formats/:driver/:fingerprint",
        method = "get"
    )]
    async fn supported_formats(
        &self,
        driver: poem_openapi::param::Path<String>,
        fingerprint: poem_openapi::param::Path<String>,
    ) -> Json<Vec<CameraResolution>> {
        tracing::info!(
            "API: supported_formats driver={} fingerprint={}",
            driver.0,
            fingerprint.0
        );

        if driver.0 != "usb_camera" {
            return Json(vec![]);
        }

        match get_supported_formats(&fingerprint.0) {
            Some(formats) => Json(formats),
            None => Json(vec![]),
        }
    }
}
