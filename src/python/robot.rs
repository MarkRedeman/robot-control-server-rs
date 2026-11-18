//! PyO3 binding for [`FeetechRobotClient`].

use numpy::{PyArray1, PyReadonlyArray1};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::robots::{ArmCalibration, FeetechRobotClient, JointUnit};

fn to_pyerr(e: anyhow::Error) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

/// A Rust-backed SO-101 robot driver.
#[pyclass(name = "RustRobot")]
pub struct RustRobot {
    inner: FeetechRobotClient,
}

#[pymethods]
impl RustRobot {
    /// Create a robot driver. `calibration` is a lerobot calibration object as
    /// a JSON string, or `None`. `unit` is `"normalized"`, `"degrees"`, or
    /// `"ticks"`.
    #[new]
    #[pyo3(signature = (port, baudrate, unit, calibration=None))]
    fn new(
        port: String,
        baudrate: u32,
        unit: String,
        calibration: Option<String>,
    ) -> PyResult<Self> {
        let calibration = calibration
            .map(|s| serde_json::from_str::<ArmCalibration>(&s))
            .transpose()
            .map_err(|e| PyRuntimeError::new_err(format!("Invalid calibration JSON: {e}")))?;
        let unit = unit.parse::<JointUnit>().map_err(PyRuntimeError::new_err)?;
        let inner = FeetechRobotClient::new(port, baudrate, calibration, unit).map_err(to_pyerr)?;
        Ok(Self { inner })
    }

    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| self.inner.connect()).map_err(to_pyerr)
    }

    fn disconnect(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| self.inner.disconnect())
            .map_err(to_pyerr)
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn device_id(&self) -> String {
        self.inner.device_id()
    }

    fn joint_names(&self) -> Vec<String> {
        self.inner
            .joint_names()
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    fn get_observation<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let observation = py
            .allow_threads(|| self.inner.get_observation())
            .map_err(to_pyerr)?;
        Ok(PyArray1::from_vec(py, observation.joint_positions))
    }

    fn send_action<'py>(
        &self,
        py: Python<'py>,
        action: PyReadonlyArray1<'py, f64>,
    ) -> PyResult<()> {
        let values = action
            .as_slice()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .to_vec();
        py.allow_threads(|| self.inner.send_action(&values))
            .map_err(to_pyerr)
    }

    fn set_torque(&self, py: Python<'_>, enabled: bool) -> PyResult<()> {
        py.allow_threads(|| self.inner.set_torque(enabled))
            .map_err(to_pyerr)
    }
}
