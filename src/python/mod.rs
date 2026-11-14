//! Python bindings for the robot and camera drivers.

mod camera;
mod robot;

use pyo3::prelude::*;

/// Native extension module imported by the `physicalai_so101_rust_plugin`
/// package. All non-trivial behavior lives in the Rust library; this module is
/// a thin type adapter.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<robot::RustRobot>()?;
    m.add_class::<camera::RustCamera>()?;
    m.add_function(wrap_pyfunction!(discover_cameras, m)?)?;
    m.add_function(wrap_pyfunction!(query_camera_formats, m)?)?;
    Ok(())
}

/// List available UVC cameras as ``(device_id, index, name)`` tuples.
#[pyfunction]
fn discover_cameras() -> Vec<(String, u32, String)> {
    crate::cameras::discover()
        .into_iter()
        .map(|d| (d.device_id, d.index, d.name))
        .collect()
}

/// Query supported ``(width, height, fps)`` formats for a camera index.
#[pyfunction]
fn query_camera_formats(index: u32) -> Vec<(u32, u32, u32)> {
    crate::cameras::query_formats(index)
        .into_iter()
        .map(|f| (f.width, f.height, f.fps))
        .collect()
}
