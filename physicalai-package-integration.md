# PhysicalAI package integration

Goal: turn `robot-control-server` into a PhysicalAI Studio plugin — a Python
wheel that binds to the existing Rust robot/camera code and registers
Rust-backed `Robot` and `Camera` implementations with Studio.

The Rust library owns all non-trivial behavior (unit conversion, connection and
torque lifecycle, observation/action shaping, camera capture and decode). The
Python layer is a thin adapter that only translates types (NumPy arrays,
`RobotObservation`/`Frame` dataclasses) and registers with Studio.

## Decisions

- **Package**: `physicalai-so101-rust-plugin`
  - Python module: `physicalai_so101_rust_plugin`
  - Studio catalog types: `SO101Rust_Follower`, `SO101Rust_Leader`
  - Driver class: `SO101Rust`
  - Camera class: `SO101RustUVCCamera`
- **Rust layout**: single crate, PyO3 bindings behind a `python` feature
  (`crate-type = ["cdylib", "rlib"]`).
- **Camera**: UVC only, via the existing nokhwa/v4l2 logic.
- **Units**: full parity — `normalized`, `degrees`, `ticks`. Unit is a
  constructor argument (no getters/setters).

## Phase 1 — Rust

### 1a. Unit + observation + driver types (`src/robots/`)

- `units.rs`: `enum JointUnit { Normalized, Degrees, Ticks }` with `as_str()`
  and `FromStr`.
- `driver.rs`: `struct RobotObservation { joint_positions: Vec<f64> }` and the
  integration trait:

```rust
pub trait RobotDriver: Send + Sync {
    fn joint_names(&self) -> &'static [&'static str];
    fn device_id(&self) -> String;                 // "serial:<basename>", no hardware I/O
    fn connect(&self) -> Result<()>;               // idempotent
    fn disconnect(&self) -> Result<()>;            // idempotent, safe stop
    fn is_connected(&self) -> bool;
    fn get_observation(&self) -> Result<RobotObservation>;
    fn send_action(&self, action: &[f64]) -> Result<()>;
    fn set_torque(&self, enabled: bool) -> Result<()>;
}
```

### 1b. Refactor `FeetechRobotClient` (`src/robots/feetech.rs`)

- Constructor `new(port, baudrate, calibration, unit)` stores config only —
  **no port open** — and validates the unit/calibration combination
  (degrees/normalized require calibration; ticks requires uncalibrated).
- Connection state becomes `controller: Mutex<Option<Sts3215Controller>>`.
- `connect()` opens the port and creates the controller (idempotent).
- `disconnect()` disables torque and drops the controller (idempotent).
- `is_connected()` reflects `Option` state.
- Implement `RobotDriver`; wire `calibration::calibrated_degrees` /
  `unnormalize_degrees` into read/write so `Degrees` works (gripper stays
  percentage in all calibrated modes, matching the SO-101 convention).
- Keep the server-facing `RobotClient` trait and `ArmState`/`JointState`
  intact; `FeetechRobotClient` implements both traits.

### 1c. Update server/CLI for the new constructor

- `state.rs` and `bin/cli.rs` call `client.connect()` after construction and
  pass the appropriate `JointUnit` (normalized when calibrated, ticks otherwise).

### 1d. Camera driver (`src/cameras/`)

- Add a synchronous nokhwa driver used by Python (additive; the tokio
  `CameraWorker` for the server stays as-is):

```rust
pub struct FrameData { pub width: u32, pub height: u32, pub data: Vec<u8> /* RGB */ }
pub trait CameraDriver {
    fn device_id(&self) -> String;
    fn connect(&mut self) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
    fn read(&mut self) -> Result<FrameData>;
    fn read_latest(&mut self) -> Result<FrameData>;
}
```

plus `discover()` and `query_formats(device_id)` (MJPEG → RGB via the `image`
crate).

### 1e. Rust tests

- Unit tests for unit round-trips (normalized/degrees/ticks) against lerobot
  calibration fixtures. Keep `cargo test` green.

## Phase 2 — Python (minimal wrapper)

### 2a. Build config

- Root `pyproject.toml` with maturin backend:
  - `[tool.maturin] python-source = "python"`,
    `module-name = "physicalai_so101_rust_plugin._native"`, `features = ["python"]`
  - `[project.entry-points."physicalai.studio.catalog_plugins"]`
  - deps: `physicalai>=0.4.0`, `physicalai-studio-plugin>=0.3.0`, `numpy`,
    `pyserial`, `pydantic`, `loguru`.

### 2b. PyO3 shim (`src/python/`, feature-gated)

- `_native` module exposing two thin classes mapping 1:1 to the Rust drivers:
  - `RustRobot` → `FeetechRobotClient`; `get_observation` returns float32
    NumPy arrays, `send_action` accepts arrays, exposes
    `connect/disconnect/is_connected/set_torque/joint_names/device_id`.
  - `RustCamera` → sync camera driver; `read()` returns
    `(RGB uint8 ndarray, timestamp, sequence)`.
  - All I/O wrapped in `py.allow_threads`.

### 2c. Python package (`python/physicalai_so101_rust_plugin/`)

- `robot.py`: `SO101Rust(Robot)` + `SO101RustObservation` — thin delegation;
  `get_observation` wraps native positions + `time.monotonic()`.
- `camera.py`: `SO101RustUVCCamera(Camera)` — thin delegation wrapping native
  `read` into `Frame`.
- `calibration.py`: lerobot JSON ↔ dict dataclasses with `to_config_value` for
  `@export_config` round-trips.
- `studio_catalog.py`: payload model, `RobotCatalogDefinition`s, `robot_builder`,
  `RobotProbe` (serial discovery + gripper identify), `RobotAsset`.
- `_urdf.py` + vendored `urdf/`.
- `__init__.py` lazy re-exports.

### 2d. Python tests

- Mock the compiled `_native` module so tests run without hardware; assert
  `Robot`/`Camera` protocol conformance, unit round-trips, `verify_robot` shapes.

## Out of scope / follow-ups

- Fully unifying the server onto `RobotDriver` (server keeps `RobotClient`).
- IP camera support.
